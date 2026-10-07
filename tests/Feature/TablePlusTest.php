<?php

use App\Models\Connection;
use App\Support\Argv;
use App\TablePlus\Keychain;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();

    $this->keychain = new class extends Keychain
    {
        public array $asked = [];

        public array $stored = [
            'prod-1_database' => 'db-secret',
            'jump-1_database' => 'jump-db-secret',
            'jump-1_server' => 'ssh-secret',
        ];

        public function password(string $account): ?string
        {
            $this->asked[] = $account;

            return $this->stored[$account] ?? null;
        }
    };

    app()->instance(Keychain::class, $this->keychain);
});

function plistValue(mixed $value): string
{
    return match (true) {
        is_bool($value) => $value ? '<true/>' : '<false/>',
        is_int($value) => "<integer>{$value}</integer>",
        is_array($value) && array_is_list($value) => '<array>'.implode('', array_map(plistValue(...), $value)).'</array>',
        is_array($value) => '<dict>'.implode('', array_map(
            fn (string $key, mixed $item) => '<key>'.htmlspecialchars($key).'</key>'.plistValue($item),
            array_keys($value),
            $value,
        )).'</dict>',
        default => '<string>'.htmlspecialchars((string) $value).'</string>',
    };
}

function tablePlusFolder(array $connections, array $groups = []): string
{
    $directory = sys_get_temp_dir().'/tql-tableplus-'.uniqid();
    mkdir($directory);

    $wrap = fn (array $items) => '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0">'.plistValue($items).'</plist>';

    file_put_contents($directory.'/Connections.plist', $wrap($connections));
    file_put_contents($directory.'/ConnectionGroups.plist', $wrap($groups));

    return $directory;
}

function tablePlusConnection(array $overrides): array
{
    return [
        'ID' => 'id-'.uniqid(),
        'ConnectionName' => 'unnamed',
        'Driver' => 'MySQL',
        'DatabaseHost' => '10.0.0.5',
        'DatabasePort' => '3306',
        'DatabaseName' => 'app',
        'DatabaseUser' => 'forge',
        'DatabasePasswordMode' => 0,
        'ServerPasswordMode' => 0,
        'Enviroment' => '',
        'GroupID' => '',
        'isOverSSH' => false,
        'isUsePrivateKey' => false,
        'ServerAddress' => '',
        'ServerPort' => '',
        'ServerUser' => '',
        'ServerPrivateKeyName' => '',
        'TlsKeyPaths' => ['', '', ''],
        'tLSMode' => 0,
        ...$overrides,
    ];
}

it('brings TablePlus connections over with their groups, tags and passwords', function () {
    $folder = tablePlusFolder([
        tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'NotaryDash Prod', 'Enviroment' => 'production', 'GroupID' => 'g-1', 'tLSMode' => 2]),
        tablePlusConnection([
            'ID' => 'jump-1', 'ConnectionName' => 'Behind SSH', 'DatabaseHost' => '127.0.0.1',
            'isOverSSH' => true, 'ServerAddress' => '203.0.113.7', 'ServerPort' => '2222', 'ServerUser' => 'forge',
        ]),
    ], [['ID' => 'g-1', 'Name' => 'NotaryDash', 'GroupID' => '', 'IsExpaned' => true]]);

    $this->artisan('tableplus', ['--from' => $folder, '--table' => true])->assertExitCode(0);

    $prod = Connection::where('name', 'NotaryDash Prod')->first();
    $ssh = Connection::where('name', 'Behind SSH')->first();

    expect($prod)
        ->group_name->toBe('NotaryDash')
        ->tag->toBe('production')
        ->ssl_mode->toBe('require')
        ->password->toBe('db-secret')
        ->and($ssh)
        ->ssh_host->toBe('203.0.113.7')
        ->ssh_port->toBe(2222)
        ->ssh_user->toBe('forge')
        ->ssh_password->toBe('ssh-secret')
        ->password->toBe('jump-db-secret');
});

it('touches nothing and asks the Keychain nothing on a dry run', function () {
    $folder = tablePlusFolder([tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'NotaryDash Prod'])]);

    $this->artisan('tableplus', ['--from' => $folder, '--dry-run' => true, '--table' => true])
        ->expectsOutputToContain('1 to bring over')
        ->assertExitCode(0);

    expect(Connection::count())->toBe(0)
        ->and($this->keychain->asked)->toBe([]);
});

it('leaves passwords in the Keychain when asked to', function () {
    $folder = tablePlusFolder([tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'NotaryDash Prod'])]);

    $this->artisan('tableplus', ['--from' => $folder, '--without-passwords' => true, '--table' => true])->assertExitCode(0);

    expect(Connection::first()->password)->toBeNull()
        ->and($this->keychain->asked)->toBe([]);
});

it('does not look in the Keychain for a connection TablePlus asks about every time', function () {
    $folder = tablePlusFolder([tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'Asks', 'DatabasePasswordMode' => 1])]);

    $this->artisan('tableplus', ['--from' => $folder, '--table' => true])->assertExitCode(0);

    expect($this->keychain->asked)->toBe([])
        ->and(Connection::first()->password)->toBeNull();
});

it('skips what tql cannot open and what it already has, so running it twice is safe', function () {
    $folder = tablePlusFolder([
        tablePlusConnection(['ConnectionName' => 'Cache', 'Driver' => 'Redis']),
        tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'NotaryDash Prod']),
    ]);

    $this->artisan('tableplus', ['--from' => $folder, '--table' => true])->assertExitCode(0);
    $this->artisan('tableplus', ['--from' => $folder, '--table' => true])
        ->expectsOutputToContain('already in tql as NotaryDash Prod')
        ->expectsOutputToContain('tql does not speak Redis')
        ->assertExitCode(0);

    expect(Connection::count())->toBe(1);
});

it('answers an agent in JSON without a password in it', function () {
    $folder = tablePlusFolder([tablePlusConnection(['ID' => 'prod-1', 'ConnectionName' => 'NotaryDash Prod', 'Enviroment' => 'production'])]);

    Artisan::call('tableplus', ['--from' => $folder, '--json' => true]);
    $output = Artisan::output();
    $report = json_decode($output, true);

    expect($report['imported'][0])->toMatchArray([
        'name' => 'NotaryDash Prod',
        'driver' => 'mysql',
        'tag' => 'production',
        'password' => 'from Keychain',
    ])->and($output)->not->toContain('db-secret');
});

it('reads a sqlite connection from its path', function () {
    $folder = tablePlusFolder([tablePlusConnection(['ConnectionName' => 'Local file', 'Driver' => 'SQLite', 'DatabasePath' => '/tmp/app.sqlite'])]);

    $this->artisan('tableplus', ['--from' => $folder, '--table' => true])->assertExitCode(0);

    expect(Connection::first())->driver->toBe('sqlite')->database->toBe('/tmp/app.sqlite');
});

it('is not mistaken for a file to open', function () {
    expect(Argv::rewrite(['tql', 'tableplus']))->toBe(['tql', 'tableplus']);
});
