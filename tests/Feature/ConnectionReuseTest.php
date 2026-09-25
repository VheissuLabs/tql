<?php

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\Event;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

function reusable(): Connection
{
    $path = sys_get_temp_dir().'/tql-reuse-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text)');

    return Connection::create(['name' => 'reuse'.uniqid(), 'driver' => 'sqlite', 'database' => $path]);
}

function queriesOn(Connection $connection, callable $do): int
{
    $count = 0;

    Event::listen(QueryExecuted::class, function (QueryExecuted $event) use (&$count) {
        if (str_starts_with($event->connectionName, 'tql_target_')) {
            $count++;
        }
    });

    $do();

    return $count;
}

it('keeps one open connection rather than reconnecting for every query', function () {
    $connection = reusable();
    $manager = app(ConnectionManager::class);

    $first = $manager->resolve($connection)->getPdo();
    $second = $manager->resolve($connection)->getPdo();

    expect($second)->toBe($first);
});

it('reconnects when the connection it points at changes', function () {
    $connection = reusable();
    $manager = app(ConnectionManager::class);

    $first = $manager->resolve($connection)->getPdo();

    $other = sys_get_temp_dir().'/tql-reuse-'.uniqid().'.sqlite';
    touch($other);
    $connection->database = $other;

    $second = $manager->resolve($connection);

    expect($second->getPdo())->not->toBe($first)
        ->and($second->getDatabaseName())->toBe($other);
});

it('asks the database for a primary key once, not on every call', function () {
    $connection = reusable();
    $runner = app(QueryRunner::class);

    $count = queriesOn($connection, function () use ($runner, $connection) {
        foreach (range(1, 5) as $ignored) {
            expect($runner->primaryKey($connection, 'widgets'))->toBe('id');
        }
    });

    expect($count)->toBe(1);
});

it('asks again for a primary key after the schema is forgotten', function () {
    $connection = reusable();
    $runner = app(QueryRunner::class);

    $runner->primaryKey($connection, 'widgets');
    $runner->forgetSchema();

    $count = queriesOn($connection, fn () => $runner->primaryKey($connection, 'widgets'));

    expect($count)->toBe(1);
});
