<?php

use App\Database\SqlExporter;
use App\Models\Connection;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

function exportFixture(): array
{
    $path = sys_get_temp_dir().'/tql-export-'.uniqid().'.sqlite';
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

    $target = sys_get_temp_dir().'/tql-import-'.uniqid().'.sqlite';
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

    expect($contents)->toContain('-- tql export')
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

it('exports every table into one file', function () {
    [$connection, $path, $pdo] = exportFixture();

    $pdo->exec('create table others (id integer primary key, label text)');
    $pdo->exec("insert into others (label) values ('one'), ('two')");

    $result = app(SqlExporter::class)->tables($connection, ['things', 'others']);
    $contents = file_get_contents($result->path);

    expect($result->rows)->toBe(6)
        ->and($contents)->toContain('-- things')
        ->and($contents)->toContain('-- others')
        ->and(substr_count($contents, '-- tql export'))->toBe(1);

    unlink($result->path);
    unlink($path);
});

it('honours a row limit', function () {
    [$connection, $path] = exportFixture();

    $result = app(SqlExporter::class)->table($connection, 'things', null, 2);

    expect($result->rows)->toBe(2);

    unlink($result->path);
    unlink($path);
});

it('exports from the console command', function () {
    [$connection, $path] = exportFixture();

    $out = sys_get_temp_dir().'/tql-cmd-'.uniqid().'.sql';

    $this->artisan('export', ['connection' => $connection->name, 'table' => 'things', '--sql' => $out])
        ->assertExitCode(0);

    expect(file_exists($out))->toBeTrue()
        ->and(file_get_contents($out))->toContain('insert into');

    unlink($out);
    unlink($path);
});

it('fails clearly for an unknown connection', function () {
    $this->artisan('export', ['connection' => 'definitely-not-a-connection'])
        ->assertExitCode(1);
});

it('fails clearly for an unknown table', function () {
    [$connection, $path] = exportFixture();

    $this->artisan('export', ['connection' => $connection->name, 'table' => 'nope'])
        ->assertExitCode(1);

    unlink($path);
});

it('asks which connection and table when given neither', function () {
    Connection::query()->delete();

    [$connection, $path] = exportFixture();

    $out = sys_get_temp_dir().'/tql-ask-'.uniqid().'.sql';

    $this->artisan('export')
        ->expectsQuestion('Export from', $connection->id)
        ->expectsQuestion('Which table?', 'things')
        ->expectsQuestion('Save it where?', $out)
        ->assertExitCode(0);

    expect(file_get_contents($out))->toContain('insert into "things"');

    unlink($out);
    unlink($path);
});

it('takes every table when the table list is answered with the first option', function () {
    Connection::query()->delete();

    [$connection, $path] = exportFixture();

    $out = sys_get_temp_dir().'/tql-ask-all-'.uniqid().'.sql';

    // The first option on the table list is the whole database.
    $this->artisan('export')
        ->expectsQuestion('Export from', $connection->id)
        ->expectsQuestion('Which table?', '')
        ->expectsQuestion('Save it where?', $out)
        ->assertExitCode(0);

    expect(file_get_contents($out))->toContain('insert into');

    unlink($out);
    unlink($path);
});

it('says which connections there are when it cannot ask', function () {
    [$connection, $path] = exportFixture();

    $this->artisan('export', ['--no-interaction' => true])
        ->expectsOutputToContain($connection->name)
        ->assertExitCode(1);

    unlink($path);
});

it('has nothing to export before a connection is saved', function () {
    Connection::query()->delete();

    $this->artisan('export', ['--no-interaction' => true])
        ->expectsOutputToContain('No saved connections')
        ->assertExitCode(1);
});

it('names the file itself when the answer is a folder', function () {
    Connection::query()->delete();

    [$connection, $path] = exportFixture();

    $folder = sys_get_temp_dir().'/tql-into-'.uniqid();
    mkdir($folder);

    $this->artisan('export')
        ->expectsQuestion('Export from', $connection->id)
        ->expectsQuestion('Which table?', 'things')
        ->expectsQuestion('Save it where?', $folder)
        ->assertExitCode(0);

    $written = glob($folder.'/*.sql');

    expect($written)->toHaveCount(1)
        ->and(file_get_contents($written[0]))->toContain('insert into "things"');

    unlink($written[0]);
    rmdir($folder);
    unlink($path);
});

it('does not ask where when --sql says so', function () {
    Connection::query()->delete();

    [$connection, $path] = exportFixture();

    $out = sys_get_temp_dir().'/tql-told-'.uniqid().'.sql';

    $this->artisan('export', ['--sql' => $out])
        ->expectsQuestion('Export from', $connection->id)
        ->expectsQuestion('Which table?', 'things')
        ->assertExitCode(0);

    expect(file_exists($out))->toBeTrue();

    unlink($out);
    unlink($path);
});
