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

it('marks a foreign key column in the header', function () {
    $browser = linked();

    $frame = structureFrame($browser);

    expect($frame)->toContain('artist_id →')
        ->and($frame)->not->toContain('title →');
});

it('finds the tables that reference this one', function () {
    $browser = linked();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->currentTable())->toBe('artists')
        ->and($browser->backLinks())->toBe([
            ['table' => 'albums', 'column' => 'artist_id', 'references' => 'id'],
        ]);
});

it('follows a row backwards to what references it', function () {
    $browser = linked();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->currentTable())->toBe('artists')
        ->and($browser->raw[0]['name'])->toBe('AC/DC');

    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('albums')
        ->and($browser->raw)->toHaveCount(1)
        ->and($browser->raw[0]['title'])->toBe('Let There Be Rock')
        ->and($browser->status)->toContain('followed → albums where artist_id is 1');
});

it('goes back from a reverse jump', function () {
    $browser = linked();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');
    $browser->emit('key', 'L');

    expect($browser->currentTable())->toBe('albums');

    $browser->emit('key', Browser::BACK);

    expect($browser->currentTable())->toBe('artists')
        ->and($browser->filters)->toBeNull();
});

it('offers a list when more than one table references this one', function () {
    $browser = linked();

    $pdo = new PDO('sqlite:'.$browser->connection->database);
    $pdo->exec('create table gigs (id integer primary key, artist_id integer references artists(id), city text)');
    $pdo->exec("insert into gigs (artist_id, city) values (1, 'Toronto')");

    $browser->emit('key', 'r');
    $browser->focus = 'sidebar';

    while ($browser->currentTable() !== 'artists') {
        $browser->emit('key', 'j');
    }

    $browser->emit('key', 'L');

    expect($browser->linkPicker)->not->toBeNull()
        ->and($browser->linkPicker->title)->toBe('REFERENCED BY')
        ->and($browser->linkPicker->options)->toBe(['albums.artist_id', 'gigs.artist_id']);

    $browser->emit('key', 'gigs');
    $browser->emit('key', "\n");

    expect($browser->linkPicker)->toBeNull()
        ->and($browser->currentTable())->toBe('gigs')
        ->and($browser->raw[0]['city'])->toBe('Toronto');
});

it('leaves the list alone on escape', function () {
    $browser = linked();

    $pdo = new PDO('sqlite:'.$browser->connection->database);
    $pdo->exec('create table gigs (id integer primary key, artist_id integer references artists(id))');

    $browser->emit('key', 'r');
    $browser->focus = 'sidebar';

    while ($browser->currentTable() !== 'artists') {
        $browser->emit('key', 'j');
    }

    $browser->emit('key', 'L');
    $browser->emit('key', "\e");

    expect($browser->linkPicker)->toBeNull()
        ->and($browser->currentTable())->toBe('artists');
});

it('inspects a row with the record it belongs to', function () {
    $browser = linked();

    expect($browser->currentTable())->toBe('albums');

    $browser->emit('key', 'i');

    $object = json_decode($browser->cellEditor->buffer(), true);

    expect($object['title'])->toBe('Let There Be Rock')
        ->and($object['artists'])->toBeArray()
        ->and($object['artists']['name'])->toBe('AC/DC');
});

it('inspects a row with the records that belong to it', function () {
    $browser = linked();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->currentTable())->toBe('artists');

    $browser->emit('key', 'i');

    $object = json_decode($browser->cellEditor->buffer(), true);

    expect($object['name'])->toBe('AC/DC')
        ->and($object['albums'])->toHaveCount(1)
        ->and($object['albums'][0]['title'])->toBe('Let There Be Rock');
});

it('caps how many related rows it loads and says it did', function () {
    $browser = linked();

    $pdo = new PDO('sqlite:'.$browser->connection->database);

    foreach (range(1, 12) as $i) {
        $pdo->prepare('insert into albums (title, artist_id) values (?, 1)')->execute(["Album {$i}"]);
    }

    config(['tql.ui.inspect_related' => 3]);

    $browser->emit('key', 'r');
    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');
    $browser->emit('key', 'i');

    $object = json_decode($browser->cellEditor->buffer(), true);

    expect($object['albums'])->toHaveCount(3)
        ->and($object['albums_truncated_at'])->toBe(3);

    config(['tql.ui.inspect_related' => 10]);
});

it('loads no relations when the limit is zero', function () {
    config(['tql.ui.inspect_related' => 0]);

    $browser = linked();

    $browser->emit('key', 'i');

    $object = json_decode($browser->cellEditor->buffer(), true);

    expect($object)->not->toHaveKey('artists')
        ->and($object['title'])->toBe('Let There Be Rock');

    config(['tql.ui.inspect_related' => 10]);
});

it('leaves a null foreign key without a relation', function () {
    $browser = linked();

    $pdo = new PDO('sqlite:'.$browser->connection->database);
    $pdo->exec("insert into albums (title, artist_id) values ('Orphan', null)");

    $browser->emit('key', 'r');
    $browser->emit('key', 'G');

    while (($browser->raw[$browser->rowIndex]['title'] ?? '') !== 'Orphan') {
        $browser->emit('key', 'j');
    }

    $browser->emit('key', 'i');

    $object = json_decode($browser->cellEditor->buffer(), true);

    expect($object['title'])->toBe('Orphan')
        ->and($object)->not->toHaveKey('artists');
});
