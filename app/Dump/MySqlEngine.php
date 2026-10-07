<?php

namespace App\Dump;

use App\Database\ConnectionManager;
use App\Models\Connection;
use App\Support\Paths;

class MySqlEngine implements Engine
{
    private const SSL_MODES = [
        'disable' => 'DISABLED',
        'prefer' => 'PREFERRED',
        'require' => 'REQUIRED',
        'verify-ca' => 'VERIFY_CA',
        'verify-full' => 'VERIFY_IDENTITY',
    ];

    public function __construct(
        private ConnectionManager $connections,
        private Secrets $secrets,
    ) {}

    public function name(): string
    {
        return 'mysql';
    }

    public function dumper(): ?string
    {
        return 'mydumper';
    }

    public function loader(): ?string
    {
        return 'myloader';
    }

    public function install(): string
    {
        return PHP_OS_FAMILY === 'Darwin'
            ? 'brew install mydumper'
            : 'install mydumper from your package manager, or from https://github.com/mydumper/mydumper/releases';
    }

    public function dump(Connection $connection, string $directory, DumpOptions $options): Job
    {
        $database = (string) $connection->activeDatabase();

        $command = [
            'mydumper',
            '--defaults-file='.$this->clientFile($connection),
            '--database='.$database,
            '--outputdir='.$directory,
            '--threads='.$options->threads,
            '--triggers',
            '--routines',
            '--skip-definer',
            '--verbose=3',
        ];

        if ($options->tables !== []) {
            $command[] = '--tables-list='.implode(',', array_map(
                fn (string $table) => $database.'.'.$table,
                $options->tables,
            ));
        }

        if ($options->dataOnly) {
            $command[] = '--no-schemas';
        }

        if ($options->noLock) {
            $command[] = '--sync-thread-lock-mode=NO_LOCK';
        }

        return new Job(
            $command,
            progress: Progress::countingTables('/`[^`]+`\.`([^`]+)` \[\s*\d+%\s*\]/'),
            tables: fn () => $this->tableCount($connection, $options->tables),
        );
    }

    public function load(Connection $connection, string $directory, LoadOptions $options): Job
    {
        $command = [
            'myloader',
            '--defaults-file='.$this->clientFile($connection),
            '--directory='.$directory,
            '--database='.$connection->activeDatabase(),
            '--threads='.$options->threads,
            '--verbose=3',
        ];

        if ($options->drop) {
            $command[] = '--drop-table=DROP';
        }

        return new Job(
            $command,
            progress: Progress::reportedTables(
                '/Tables (\d+) of (\d+) completed/',
                '/restoring (?:table |indexes )?[^.\s]+\.(\S+)/',
            ),
            tables: fn () => count(preg_grep('/^[^.]+\.[^.]+-schema\.sql/', scandir($directory) ?: [])),
        );
    }

    public function estimatedBytes(Connection $connection, array $tables): ?int
    {
        $query = 'select coalesce(sum(data_length), 0) as bytes from information_schema.tables where table_schema = ?';
        $bindings = [$connection->activeDatabase()];

        if ($tables !== []) {
            $query .= ' and table_name in ('.implode(', ', array_fill(0, count($tables), '?')).')';
            $bindings = [...$bindings, ...$tables];
        }

        return (int) $this->connections->resolve($connection)->selectOne($query, $bindings)->bytes;
    }

    private function tableCount(Connection $connection, array $tables): int
    {
        if ($tables !== []) {
            return count($tables);
        }

        return (int) $this->connections->resolve($connection)->selectOne(
            "select count(*) as tables from information_schema.tables where table_schema = ? and table_type = 'BASE TABLE'",
            [$connection->activeDatabase()],
        )->tables;
    }

    public function compresses(): bool
    {
        return false;
    }

    public function recognises(string $directory): bool
    {
        return is_file($directory.'/metadata');
    }

    public function clientFile(Connection $connection): string
    {
        $endpoint = Endpoint::of($connection);

        $settings = array_filter([
            'host' => $endpoint->host,
            'port' => (string) $endpoint->port,
            'user' => (string) $connection->username,
            'password' => (string) $connection->password,
            'ssl-mode' => self::SSL_MODES[trim((string) $connection->ssl_mode)] ?? '',
            'ssl-ca' => Paths::expand((string) $connection->ssl_ca),
            'ssl-cert' => Paths::expand((string) $connection->ssl_cert),
            'ssl-key' => Paths::expand((string) $connection->ssl_key),
        ], fn (string $value) => $value !== '');

        $lines = array_map(
            fn (string $key, string $value) => $key.'="'.addcslashes($value, '"\\').'"',
            array_keys($settings),
            $settings,
        );

        return $this->secrets->write('client.cnf', "[client]\n".implode("\n", $lines)."\n");
    }
}
