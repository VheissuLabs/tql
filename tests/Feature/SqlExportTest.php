<?php

use App\Database\SqlExporter;
use App\Models\Connection;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

function exportFixture(): array
{
    $path = sys_get_temp_dir().'/dotsql-export-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table things (id integer primary key, name text, note text, qty integer, price real)');

    $statement = $pdo->prepare('insert into things (name, note, qty, price) values (?, ?, ?, ?)');
    $statement->execute(["it's quoted", null, 1, 9.99]);
    $statement->execute(['unicode — é ü 日本', "line\nbreak", 2, 0.5]);
    $statement->execute(['back\\slash', 'semi;colon', 3, 1000.0]);
    $statement->execute([null, 'trailing space ', 0, 0.0]);

    $connection = Connection::create([
        'name' => 'export'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return [$connection, $path, $pdo];
}

it('exports a table to re-importable sql', function () {
    [$connection, $path, $pdo] = exportFixture();

    $result = app(SqlExporter::class)->table($connection, 'things');

    expect($result->rows)->toBe(4)
        ->and(file_exists($result->path))->toBeTrue()
        ->and(file_get_contents($result->path))->toContain('insert into');

    $ddl = $pdo->query("select sql from sqlite_master where name='things'")->fetchColumn();

    $target = sys_get_temp_dir().'/dotsql-import-'.uniqid().'.sqlite';
    touch($target);

    $imported = new PDO('sqlite:'.$target);
    $imported->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
    $imported->exec($ddl);
    $imported->exec(file_get_contents($result->path));

    $before = $pdo->query('select * from things order by id')->fetchAll(PDO::FETCH_ASSOC);
    $after = $imported->query('select * from things order by id')->fetchAll(PDO::FETCH_ASSOC);

    expect($after)->toBe($before);

    unlink($result->path);
    unlink($target);
    unlink($path);
});

it('writes a header naming the connection and table', function () {
    [$connection, $path] = exportFixture();

    $result = app(SqlExporter::class)->table($connection, 'things');
    $contents = file_get_contents($result->path);

    expect($contents)->toContain('-- dotsql export')
        ->and($contents)->toContain('-- table: things')
        ->and($contents)->toContain($connection->name);

    unlink($result->path);
    unlink($path);
});

it('exports an empty table without writing insert statements', function () {
    [$connection, $path, $pdo] = exportFixture();

    $pdo->exec('create table empties (id integer primary key)');

    $result = app(SqlExporter::class)->table($connection, 'empties');

    expect($result->rows)->toBe(0)
        ->and(file_get_contents($result->path))->not->toContain('insert into');

    unlink($result->path);
    unlink($path);
});

it('names exports after the connection, table and time', function () {
    [$connection, $path] = exportFixture();

    $result = app(SqlExporter::class)->table($connection, 'things');

    expect(basename($result->path))->toMatch('/^export[a-z0-9]+-things-\d{8}-\d{6}\.sql$/');

    unlink($result->path);
    unlink($path);
});
