<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\ConnectionForm;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function onSqlite(): Browser
{
    $path = sys_get_temp_dir().'/tql-db-'.uniqid().'.sqlite';
    touch($path);

    (new PDO('sqlite:'.$path))->exec('create table widgets (id integer primary key)');

    $connection = Connection::create([
        'name' => 'db'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

it('keeps a session database off the saved record', function () {
    $connection = Connection::create([
        'name' => 'server'.uniqid(), 'driver' => 'mysql', 'host' => 'h',
        'port' => 3306, 'database' => 'shop', 'username' => 'alice',
    ]);

    expect($connection->activeDatabase())->toBe('shop');

    $connection->sessionDatabase = 'reporting';

    expect($connection->activeDatabase())->toBe('reporting')
        ->and($connection->toLaravelConfig()['database'])->toBe('reporting');

    // Saving the record — which happens on every open — must not persist it.
    $connection->forceFill(['last_used_at' => now()])->save();

    expect($connection->fresh()->database)->toBe('shop');
});

it('has no databases to list for sqlite', function () {
    $browser = onSqlite();

    expect(app(QueryRunner::class)->databases($browser->connection))->toBe([]);

    $browser->emit('key', 'b');

    expect($browser->databasePicker)->toBeNull()
        ->and($browser->status)->toContain('one file');
});

it('does not offer the database key for sqlite', function () {
    $browser = onSqlite();

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))
        ->not->toContain('b Database');
});

it('says the database is asked for when the field is left empty', function () {
    $connection = Connection::create([
        'name' => 'empty'.uniqid(), 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
    ]);

    $form = new ConnectionForm($connection);

    expect($form->display('database'))->toBe('ask on connect');
});

it('leaves a sqlite path alone', function () {
    $connection = Connection::create([
        'name' => 'lite'.uniqid(), 'driver' => 'sqlite', 'database' => '',
    ]);

    $form = new ConnectionForm($connection);

    expect($form->display('database'))->toBe('');
});

it('returns no databases when the server cannot be reached', function () {
    $connection = new Connection([
        'name' => 'unreachable', 'driver' => 'mysql',
        'host' => '127.0.0.1', 'port' => 1, 'database' => 'shop',
    ]);

    expect(app(QueryRunner::class)->databases($connection))->toBe([]);
});

it('says so when the server offers no databases to pick', function () {
    $browser = onSqlite();

    // A server connection that answers with nothing: the picker stays shut and
    // the status line says why, rather than opening an empty list.
    $browser->connection = new Connection([
        'name' => 'server', 'driver' => 'mysql',
        'host' => '127.0.0.1', 'port' => 1, 'database' => '',
    ]);

    $browser->openDatabases();

    expect($browser->databasePicker)->toBeNull()
        ->and($browser->status)->toContain('no databases');
});
