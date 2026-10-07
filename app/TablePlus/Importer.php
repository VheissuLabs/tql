<?php

namespace App\TablePlus;

use App\Models\Connection;
use App\Support\Paths;
use RuntimeException;

class Importer
{
    public const DIRECTORIES = [
        '~/Library/Application Support/com.tinyapp.TablePlus/Data',
        '~/Library/Application Support/com.tinyapp.TablePlus-setapp/Data',
    ];

    private const DRIVERS = [
        'MySQL' => 'mysql',
        'MariaDB' => 'mysql',
        'PostgreSQL' => 'pgsql',
        'SQLite' => 'sqlite',
        'SQLServer' => 'sqlsrv',
    ];

    private const DEFAULT_PORTS = [
        'mysql' => 3306,
        'pgsql' => 5432,
        'sqlsrv' => 1433,
    ];

    private const TLS_MODES = [
        'MySQL' => ['', 'disable', 'require', 'verify-ca', 'verify-full'],
        'MariaDB' => ['', 'require', 'verify-full'],
        'PostgreSQL' => ['', 'disable', 'require', '', 'verify-ca', 'verify-full'],
    ];

    private const ENVIRONMENTS = [
        'production' => 'production',
        'staging' => 'staging',
        'development' => 'dev',
        'testing' => 'dev',
        'local' => 'local',
    ];

    private const STORED_IN_KEYCHAIN = 0;

    public function __construct(private Keychain $keychain) {}

    public static function find(): ?string
    {
        foreach (self::DIRECTORIES as $directory) {
            $expanded = Paths::expand($directory);

            if (is_file($expanded.'/Connections.plist')) {
                return $expanded;
            }
        }

        return null;
    }

    public function plan(string $directory): array
    {
        $connections = Plist::read($directory.'/Connections.plist');

        if (! is_array($connections)) {
            throw new RuntimeException("{$directory}/Connections.plist does not list any connections.");
        }

        $groups = $this->groups($directory);

        return array_map(fn (array $entry) => $this->entry($entry, $groups), $connections);
    }

    public function import(array $plan, bool $withPasswords): array
    {
        return array_map(function (array $entry) use ($withPasswords) {
            if ($entry['skip'] !== null) {
                return $entry;
            }

            $attributes = $entry['attributes'];

            if ($withPasswords) {
                $attributes = [...$attributes, ...$this->passwords($entry)];
            }

            $entry['password'] = $this->passwordState($entry, $attributes, $withPasswords);

            $connection = Connection::create([
                ...$attributes,
                'name' => Connection::freeName($entry['name']),
            ]);

            return [...$entry, 'name' => $connection->name];
        }, $plan);
    }

    private function entry(array $entry, array $groups): array
    {
        $name = trim((string) ($entry['ConnectionName'] ?? '')) ?: 'TablePlus connection';
        $tablePlusDriver = (string) ($entry['Driver'] ?? '');
        $driver = self::DRIVERS[$tablePlusDriver] ?? null;

        $planned = [
            'name' => $name,
            'id' => (string) ($entry['ID'] ?? ''),
            'driver' => $driver ?? $tablePlusDriver,
            'group' => $groups[$entry['GroupID'] ?? ''] ?? null,
            'tag' => self::ENVIRONMENTS[strtolower((string) ($entry['Enviroment'] ?? ''))] ?? null,
            'database_password_mode' => (int) ($entry['DatabasePasswordMode'] ?? self::STORED_IN_KEYCHAIN),
            'server_password_mode' => (int) ($entry['ServerPasswordMode'] ?? self::STORED_IN_KEYCHAIN),
            'over_ssh' => (bool) ($entry['isOverSSH'] ?? false),
            'ssh_key' => (bool) ($entry['isUsePrivateKey'] ?? false),
            'skip' => null,
            'password' => null,
            'attributes' => [],
        ];

        if ($driver === null) {
            return [...$planned, 'skip' => "tql does not speak {$tablePlusDriver}"];
        }

        $attributes = $this->attributes($entry, $driver, $tablePlusDriver, $planned);
        $existing = Connection::samePlaceAs($attributes);

        if ($existing !== null) {
            return [...$planned, 'skip' => "already in tql as {$existing->name}"];
        }

        return [...$planned, 'attributes' => $attributes];
    }

    private function attributes(array $entry, string $driver, string $tablePlusDriver, array $planned): array
    {
        if ($driver === 'sqlite') {
            return array_filter([
                'driver' => 'sqlite',
                'database' => (string) ($entry['DatabasePath'] ?? ''),
                'tag' => $planned['tag'],
                'group_name' => $planned['group'],
            ], fn ($value) => $value !== null);
        }

        $tlsPaths = array_values((array) ($entry['TlsKeyPaths'] ?? []));

        return array_filter([
            'driver' => $driver,
            'host' => trim((string) ($entry['DatabaseHost'] ?? '')) ?: '127.0.0.1',
            'port' => (int) ($entry['DatabasePort'] ?? 0) ?: self::DEFAULT_PORTS[$driver],
            'database' => trim((string) ($entry['DatabaseName'] ?? '')) ?: null,
            'username' => trim((string) ($entry['DatabaseUser'] ?? '')) ?: null,
            'tag' => $planned['tag'],
            'group_name' => $planned['group'],
            'ssl_mode' => self::TLS_MODES[$tablePlusDriver][(int) ($entry['tLSMode'] ?? 0)] ?? null,
            'ssl_key' => ($tlsPaths[0] ?? '') ?: null,
            'ssl_cert' => ($tlsPaths[1] ?? '') ?: null,
            'ssl_ca' => ($tlsPaths[2] ?? '') ?: null,
            ...$this->ssh($entry, $planned),
        ], fn ($value) => $value !== null && $value !== '');
    }

    private function ssh(array $entry, array $planned): array
    {
        if (! $planned['over_ssh']) {
            return [];
        }

        return [
            'ssh_host' => trim((string) ($entry['ServerAddress'] ?? '')) ?: null,
            'ssh_port' => (int) ($entry['ServerPort'] ?? 0) ?: null,
            'ssh_user' => trim((string) ($entry['ServerUser'] ?? '')) ?: null,
            'ssh_key' => $planned['ssh_key']
                ? $this->keyPath((string) ($entry['ServerPrivateKeyName'] ?? ''))
                : null,
        ];
    }

    private function keyPath(string $name): ?string
    {
        $name = trim($name);

        if ($name === '') {
            return null;
        }

        if (str_starts_with($name, '/') || str_starts_with($name, '~/')) {
            return $name;
        }

        return is_file(Paths::expand('~/.ssh/'.$name))
            ? '~/.ssh/'.$name
            : null;
    }

    private function groups(string $directory): array
    {
        $path = $directory.'/ConnectionGroups.plist';

        if (! is_file($path)) {
            return [];
        }

        $groups = [];

        foreach ((array) Plist::read($path) as $group) {
            if (isset($group['ID'], $group['Name']) && trim((string) $group['Name']) !== '') {
                $groups[(string) $group['ID']] = trim((string) $group['Name']);
            }
        }

        return $groups;
    }

    private function passwords(array $entry): array
    {
        if ($entry['id'] === '') {
            return [];
        }

        $passwords = [];

        if ($entry['database_password_mode'] === self::STORED_IN_KEYCHAIN && $entry['driver'] !== 'sqlite') {
            $passwords['password'] = $this->keychain->password($entry['id'].'_database');
        }

        if ($entry['over_ssh'] && ! $entry['ssh_key'] && $entry['server_password_mode'] === self::STORED_IN_KEYCHAIN) {
            $passwords['ssh_password'] = $this->keychain->password($entry['id'].'_server');
        }

        return array_filter($passwords, fn (?string $password) => $password !== null);
    }

    private function passwordState(array $entry, array $attributes, bool $withPasswords): string
    {
        return match (true) {
            $entry['driver'] === 'sqlite' => 'not needed',
            $entry['database_password_mode'] !== self::STORED_IN_KEYCHAIN => 'TablePlus asks for it each time; add it with e',
            ! $withPasswords => 'left out; add it with e',
            isset($attributes['password']) => 'from Keychain',
            default => 'not in Keychain, or not allowed; add it with e',
        };
    }
}
