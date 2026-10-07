<?php

use App\Models\Connection;
use App\Support\Argv;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();
});

function syncedSqlite(array $names): string
{
    $path = sys_get_temp_dir().'/tql-sync-'.uniqid().'.sqlite';

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text)');

    foreach ($names as $name) {
        $pdo->prepare('insert into widgets (name) values (?)')->execute([$name]);
    }

    return $path;
}

function syncedNames(string $path): array
{
    return (new PDO('sqlite:'.$path))->query('select name from widgets order by id')->fetchAll(PDO::FETCH_COLUMN);
}

it('makes one database a copy of another in one step, and cleans up after itself', function () {
    Connection::create(['name' => 'prod', 'driver' => 'sqlite', 'database' => syncedSqlite(['live', 'data'])]);
    $local = syncedSqlite(['stale']);
    Connection::create(['name' => 'local', 'driver' => 'sqlite', 'database' => $local]);
    $via = sys_get_temp_dir().'/tql-sync-via-'.uniqid();

    $this->artisan('sync', ['from' => 'prod', 'to' => 'local', '--via' => $via])->assertExitCode(0);

    expect(syncedNames($local))->toBe(['live', 'data'])
        ->and(is_dir($via))->toBeFalse();
});

it('keeps the dump when asked to', function () {
    Connection::create(['name' => 'prod', 'driver' => 'sqlite', 'database' => syncedSqlite(['live'])]);
    Connection::create(['name' => 'local', 'driver' => 'sqlite', 'database' => syncedSqlite([])]);
    $via = sys_get_temp_dir().'/tql-sync-via-'.uniqid();

    $this->artisan('sync', ['from' => 'prod', 'to' => 'local', '--via' => $via, '--keep' => true])->assertExitCode(0);

    expect(is_file($via.'/tql.json'))->toBeTrue();
});

it('will not copy a database onto itself', function () {
    $path = syncedSqlite(['only']);
    Connection::create(['name' => 'one', 'driver' => 'sqlite', 'database' => $path]);
    Connection::create(['name' => 'same file', 'driver' => 'sqlite', 'database' => $path]);

    $this->artisan('sync', ['from' => 'one', 'to' => 'same file'])
        ->expectsOutputToContain('onto itself')
        ->assertExitCode(1);

    expect(syncedNames($path))->toBe(['only']);
});

it('will not sync into production from a script without --force', function () {
    Connection::create(['name' => 'local', 'driver' => 'sqlite', 'database' => syncedSqlite(['dev'])]);
    $production = syncedSqlite(['live']);
    Connection::create(['name' => 'prod', 'driver' => 'sqlite', 'database' => $production, 'tag' => 'production']);

    $this->artisan('sync', ['from' => 'local', 'to' => 'prod', '--no-interaction' => true])
        ->expectsOutputToContain('tagged production')
        ->assertExitCode(1);

    expect(syncedNames($production))->toBe(['live']);
});

it('will not sync between engines', function () {
    Connection::create(['name' => 'lite', 'driver' => 'sqlite', 'database' => syncedSqlite([])]);
    Connection::create(['name' => 'server', 'driver' => 'mysql', 'host' => 'db.example.test', 'port' => 3306, 'database' => 'app']);

    $this->artisan('sync', ['from' => 'lite', 'to' => 'server'])
        ->expectsOutputToContain('does not move between engines')
        ->assertExitCode(1);
});

it('will not sync into a read only connection', function () {
    Connection::create(['name' => 'prod', 'driver' => 'sqlite', 'database' => syncedSqlite(['live'])]);
    Connection::create(['name' => 'locked', 'driver' => 'sqlite', 'database' => syncedSqlite([]), 'read_only' => true]);

    $this->artisan('sync', ['from' => 'prod', 'to' => 'locked'])
        ->expectsOutputToContain('read only')
        ->assertExitCode(1);
});

it('is not mistaken for a file to open', function () {
    expect(Argv::rewrite(['tql', 'sync', 'a', 'b']))->toBe(['tql', 'sync', 'a', 'b']);
});
