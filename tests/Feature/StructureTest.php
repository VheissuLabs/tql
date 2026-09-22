<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function linked(): Browser
{
    $path = sys_get_temp_dir().'/tql-links-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('pragma foreign_keys = on');
    $pdo->exec('create table artists (id integer primary key, name text not null)');
    $pdo->exec('create table albums (
        id integer primary key,
        title text not null default "untitled",
        artist_id integer references artists(id)
    )');
    $pdo->exec('create index albums_artist_id on albums (artist_id)');

    $pdo->exec("insert into artists (name) values ('AC/DC')");
    $pdo->exec("insert into artists (name) values ('Accept')");
    $pdo->exec("insert into albums (title, artist_id) values ('Let There Be Rock', 1)");
    $pdo->exec("insert into albums (title, artist_id) values ('Balls to the Wall', 2)");

    $connection = Connection::create([
        'name' => 'links'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $render->invoke($browser);

    return $browser;
}

function structureFrame(Browser $browser): string
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));
}

it('shows the columns and their types', function () {
    $browser = linked();

    $browser->emit('key', 't');

    expect($browser->mode)->toBe('structure');

    $frame = structureFrame($browser);

    expect($frame)->toContain('STRUCTURE')
        ->and($frame)->toContain('albums')
        ->and($frame)->toContain('title')
        ->and($frame)->toContain('integer')
        ->and($frame)->toContain('text');
});

it('marks the primary key, foreign keys and not null', function () {
    $browser = linked();

    $browser->emit('key', 't');

    $frame = structureFrame($browser);

    expect($frame)->toContain('primary key')
        ->and($frame)->toContain('→ artists.id')
        ->and($frame)->toContain('not null');
});

it('lists the indexes', function () {
    $browser = linked();

    $browser->emit('key', 't');

    expect(structureFrame($browser))->toContain('indexes')
        ->and(structureFrame($browser))->toContain('albums_artist_id');
});

it('closes with t, q or escape', function (string $key) {
    $browser = linked();

    $browser->emit('key', 't');
    expect($browser->mode)->toBe('structure');

    $browser->emit('key', $key);
    expect($browser->mode)->toBe('browse');
})->with(['t', 'q', "\e"]);

it('finds the foreign keys of the table', function () {
    $browser = linked();

    expect($browser->links())->toBe([
        'artist_id' => ['table' => 'artists', 'column' => 'id'],
    ]);
});

it('follows a foreign key to the row it points at', function () {
    $browser = linked();

    expect($browser->currentTable())->toBe('albums');

    // Move to artist_id on the first album, which points at artist 1.
    $browser->emit('key', 'l');
    $browser->emit('key', 'l');

    expect($browser->headers[$browser->columnIndex])->toBe('artist_id');

    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('artists')
        ->and($browser->raw)->toHaveCount(1)
        ->and($browser->raw[0]['name'])->toBe('AC/DC')
        ->and($browser->status)->toContain('followed artist_id → artists');
});

it('shows the jump as a where clause', function () {
    $browser = linked();

    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'L');

    expect($browser->lastStatement)->toContain('where')
        ->and($browser->lastStatement)->toContain('"id" =');
});

it('goes back with ctrl+o', function () {
    $browser = linked();

    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('artists');

    $browser->emit('key', Browser::BACK);

    expect($browser->currentTable())->toBe('albums')
        ->and($browser->filters)->toBeNull()
        ->and($browser->raw)->toHaveCount(2)
        ->and($browser->status)->toContain('back in albums');
});

it('says so when the column is not a link', function () {
    $browser = linked();

    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('albums')
        ->and($browser->status)->toContain('is not a foreign key');
});

it('says so when there is nowhere to go back to', function () {
    $browser = linked();

    $browser->emit('key', Browser::BACK);

    expect($browser->status)->toBe('nowhere to go back to');
});

it('does not follow an empty foreign key', function () {
    $browser = linked();

    $pdo = new PDO('sqlite:'.$browser->connection->database);
    $pdo->exec("insert into albums (title, artist_id) values ('Orphan', null)");

    $browser->emit('key', 'r');
    $browser->emit('key', 'G');
    $browser->emit('key', 'j');
    $browser->emit('key', 'j');
    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('albums')
        ->and($browser->status)->toContain('is empty on this row');
});
