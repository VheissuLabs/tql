<?php

use App\Dump\DumpOptions;
use App\Dump\Job;
use App\Dump\LoadOptions;
use App\Dump\Manifest;
use App\Dump\MySqlEngine;
use App\Dump\PostgresEngine;
use App\Models\Connection;
use App\Support\Argv;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();
});

function scratchDirectory(): string
{
    return sys_get_temp_dir().'/tql-dump-'.uniqid();
}

function sqliteWithWidgets(array $names = ['alpha', 'beta']): string
{
    $path = sys_get_temp_dir().'/tql-dump-'.uniqid().'.sqlite';

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text)');

    foreach ($names as $name) {
        $pdo->prepare('insert into widgets (name) values (?)')->execute([$name]);
    }

    return $path;
}

function widgetNames(string $path): array
{
    return (new PDO('sqlite:'.$path))->query('select name from widgets order by id')->fetchAll(PDO::FETCH_COLUMN);
}

function serverConnection(string $driver, array $attributes = []): Connection
{
    return Connection::create([
        'name' => $driver.'-'.uniqid(),
        'driver' => $driver,
        'host' => 'db.example.test',
        'port' => $driver === 'mysql' ? 3306 : 5432,
        'database' => 'notarydash',
        'username' => 'app',
        'password' => 'p@ss:w"rd\\1',
        ...$attributes,
    ]);
}

it('dumps a sqlite database into a folder that loads into another connection', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets()]);
    $target = sys_get_temp_dir().'/tql-dump-target-'.uniqid().'.sqlite';
    Connection::create(['name' => 'target', 'driver' => 'sqlite', 'database' => $target]);
    $directory = scratchDirectory();

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])->assertExitCode(0);

    expect(Manifest::read($directory))->toMatchArray(['engine' => 'sqlite', 'connection' => 'source']);

    $this->artisan('load', ['source' => $directory, 'connection' => 'target'])->assertExitCode(0);

    expect(widgetNames($target))->toBe(['alpha', 'beta']);
});

it('will not load over a database that is already there without --drop', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets(['fresh'])]);
    $existing = sqliteWithWidgets(['kept']);
    Connection::create(['name' => 'target', 'driver' => 'sqlite', 'database' => $existing]);
    $directory = scratchDirectory();

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])->assertExitCode(0);
    $this->artisan('load', ['source' => $directory, 'connection' => 'target'])->assertExitCode(1);

    expect(widgetNames($existing))->toBe(['kept']);

    $this->artisan('load', ['source' => $directory, 'connection' => 'target', '--drop' => true])->assertExitCode(0);

    expect(widgetNames($existing))->toBe(['fresh']);
});

it('refuses to dump into a folder that already has something in it', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets()]);
    $directory = scratchDirectory();
    mkdir($directory);
    touch($directory.'/something');

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])
        ->expectsOutputToContain('already has something in it')
        ->assertExitCode(1);
});

it('will not load a dump from one engine into another', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets()]);
    serverConnection('mysql', ['name' => 'server']);
    $directory = scratchDirectory();

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])->assertExitCode(0);

    $this->artisan('load', ['source' => $directory, 'connection' => 'server'])
        ->expectsOutputToContain('That dump is from SQLite and server is MySQL')
        ->assertExitCode(1);
});

it('will not load into a read only connection', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets()]);
    Connection::create(['name' => 'locked', 'driver' => 'sqlite', 'database' => sqliteWithWidgets(), 'read_only' => true]);
    $directory = scratchDirectory();

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])->assertExitCode(0);

    $this->artisan('load', ['source' => $directory, 'connection' => 'locked', '--drop' => true])
        ->expectsOutputToContain('read only')
        ->assertExitCode(1);
});

it('asks before loading into production and stops on no', function () {
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => sqliteWithWidgets(['fresh'])]);
    $production = sqliteWithWidgets(['live']);
    Connection::create(['name' => 'prod', 'driver' => 'sqlite', 'database' => $production, 'tag' => 'production']);
    $directory = scratchDirectory();

    $this->artisan('dump', ['connection' => 'source', '--to' => $directory])->assertExitCode(0);

    $this->artisan('load', ['source' => $directory, 'connection' => 'prod', '--drop' => true])
        ->expectsConfirmation('prod is tagged production. Load into it anyway?', 'no')
        ->assertExitCode(1);

    expect(widgetNames($production))->toBe(['live']);
});

it('replays a tql export into tables that exist, all or nothing', function () {
    $source = sqliteWithWidgets(["it's; tricky", 'plain']);
    Connection::create(['name' => 'source', 'driver' => 'sqlite', 'database' => $source]);
    $target = sqliteWithWidgets([]);
    Connection::create(['name' => 'target', 'driver' => 'sqlite', 'database' => $target]);
    $file = sys_get_temp_dir().'/tql-dump-'.uniqid().'.sql';

    $this->artisan('export', ['connection' => 'source', 'table' => 'widgets', '--sql' => $file])->assertExitCode(0);
    $this->artisan('import', ['source' => $file, 'connection' => 'target'])->assertExitCode(0);

    expect(widgetNames($target))->toBe(["it's; tricky", 'plain']);

    $this->artisan('load', ['source' => $file, 'connection' => 'target'])
        ->expectsOutputToContain('nothing was loaded')
        ->assertExitCode(1);

    expect(widgetNames($target))->toBe(["it's; tricky", 'plain']);
});

it('keeps the mysql password off the command line and in a private file', function () {
    $job = app(MySqlEngine::class)->dump(
        serverConnection('mysql'),
        '/tmp/out',
        new DumpOptions(tables: ['orders', 'users']),
    );

    $defaults = substr(collect($job->command)->first(fn (string $part) => str_starts_with($part, '--defaults-file=')), 16);

    expect($job->describe())->not->toContain('p@ss')
        ->and($job->command)->toContain('--database=notarydash', '--tables-list=notarydash.orders,notarydash.users')
        ->and(file_get_contents($defaults))->toContain('password="p@ss:w\\"rd\\\\1"', 'host="db.example.test"')
        ->and(fileperms($defaults) & 0777)->toBe(0600);
});

it('replaces tables on a mysql load only when asked', function () {
    $engine = app(MySqlEngine::class);
    $connection = serverConnection('mysql');

    expect($engine->load($connection, '/tmp/in', new LoadOptions)->command)->not->toContain('--drop-table=DROP')
        ->and($engine->load($connection, '/tmp/in', new LoadOptions(drop: true))->command)->toContain('--drop-table=DROP');
});

it('hands postgres its password through a password file', function () {
    $job = app(PostgresEngine::class)->load(
        serverConnection('pgsql', ['ssl_mode' => 'require']),
        '/tmp/in',
        new LoadOptions(drop: true),
    );

    expect($job->describe())->not->toContain('p@ss')
        ->and($job->command)->toContain('--clean', '--if-exists', '--dbname=notarydash', '/tmp/in')
        ->and($job->environment['PGSSLMODE'])->toBe('require')
        ->and(file_get_contents($job->environment['PGPASSFILE']))->toBe("*:*:*:*:p@ss\\:w\"rd\\\\1\n");
});

it('says how to install the tool when it is missing', function () {
    serverConnection('mysql', ['name' => 'server']);
    $path = getenv('PATH');

    putenv('PATH=/nonexistent');

    try {
        $this->artisan('dump', ['connection' => 'server'])
            ->expectsOutputToContain('mydumper is not installed')
            ->assertExitCode(1);
    } finally {
        putenv('PATH='.$path);
    }
});

it('does not mistake the new commands for files to open', function () {
    foreach (['dump', 'load', 'import'] as $command) {
        expect(Argv::rewrite(['tql', $command, 'x']))->toBe(['tql', $command, 'x']);
    }
});

it('gives every job its own connection file, so a dump and a load prepared together each reach their own server', function () {
    $engine = app(MySqlEngine::class);
    $source = Connection::create(['name' => 'prod', 'driver' => 'mysql', 'host' => 'prod.example.test', 'port' => 3306, 'database' => 'app', 'username' => 'reader', 'password' => 'one']);
    $target = Connection::create(['name' => 'local', 'driver' => 'mysql', 'host' => '127.0.0.1', 'port' => 3306, 'database' => 'app_copy', 'username' => 'root', 'password' => 'two']);

    $defaultsFile = fn (Job $job) => substr(collect($job->command)->first(fn (string $part) => str_starts_with($part, '--defaults-file=')), 16);

    $dump = $engine->dump($source, '/tmp/out', new DumpOptions);
    $load = $engine->load($target, '/tmp/out', new LoadOptions(drop: true));

    expect(file_get_contents($defaultsFile($dump)))->toContain('host="prod.example.test"')
        ->and(file_get_contents($defaultsFile($load)))->toContain('host="127.0.0.1"');
});
