<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowDocument;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function inspectable(): Browser
{
    $path = sys_get_temp_dir().'/tql-inspect-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, name text, payload text, created_at text)');
    $pdo->prepare('insert into events (name, payload, created_at) values (?, ?, ?)')->execute([
        'user.signed_up',
        '{"user":{"id":42,"email":"karl@example.com"}}',
        '2026-09-22T15:11:32+00:00',
    ]);

    $connection = Connection::create([
        'name' => 'inspect'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $render->invoke($browser);

    return $browser;
}

/**
 * A browser on a database built for the test, sitting on the first row.
 */
function relatedBrowser(string $sql, ?string $table = null): Browser
{
    $path = sys_get_temp_dir().'/tql-related-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);

    foreach (array_filter(array_map('trim', explode(';', $sql))) as $statement) {
        $pdo->exec($statement);
    }

    $connection = Connection::create([
        'name' => 'related'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    if ($table !== null) {
        $browser->tableIndex = (int) array_search($table, $browser->tables, true);
    }

    $browser->emit('key', "\n");
    $browser->emit('key', 'i');

    // Render once: the related rows are laid out to the width of the box, so
    // until the frame has been drawn there is no width to lay them out to.
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

function inspected(Browser $browser): string
{
    return $browser->document?->text() ?? '';
}

function inspectorLines(Browser $browser): array
{
    return array_column($browser->document->lines(), 'text');
}

it('shows the record and its types on i', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('inspect')
        ->and($browser->document)->not->toBeNull();

    $text = inspected($browser);

    expect($text)->toContain('RECORD  (4)')
        ->and($text)->toContain('name')
        ->and($text)->toContain('user.signed_up')
        ->and($text)->toContain('created_at')
        ->and($text)->toContain('text')
        ->and($text)->toContain('integer');
});

it('folds a section with enter', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect(inspected($browser))->toContain('user.signed_up');

    // The cursor starts on the record heading.
    $browser->emit('key', "\n");

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeTrue()
        ->and(inspected($browser))->not->toContain('user.signed_up');

    $browser->emit('key', "\n");

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeFalse()
        ->and(inspected($browser))->toContain('user.signed_up');
});

it('folds with space as well as enter', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', ' ');

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeTrue();
});

it('does nothing when the line is not foldable', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'j');

    $before = inspected($browser);

    $browser->emit('key', "\n");

    expect(inspected($browser))->toBe($before);
});

it('moves with j and k and jumps with g and G', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->documentLine)->toBe(0);

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($browser->documentLine)->toBe(2);

    $browser->emit('key', 'g');

    expect($browser->documentLine)->toBe(0);

    $browser->emit('key', 'G');

    expect($browser->documentLine)->toBe(count($browser->document->lines()) - 1);
});

it('keeps the cursor in range when a fold shortens the document', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'G');

    $bottom = $browser->documentLine;

    $browser->emit('key', 'g');
    $browser->emit('key', "\n");

    expect($browser->documentLine)->toBeLessThanOrEqual(count($browser->document->lines()) - 1)
        ->and($bottom)->toBeGreaterThan(0);
});

it('draws each section as its own box', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // This table has no foreign keys, so there is nothing to relate.
    expect($frame)->toContain('┌─ RECORD  (4)')
        ->and($frame)->not->toContain('RELATED');
});

it('collapses a box to its title bar', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $open = substr_count(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)), "\n");

    $browser->emit('key', "\n");

    $closed = substr_count(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)), "\n");

    // Folding takes rows off the screen, rather than emptying a box that
    // stays the same size.
    expect($closed)->toBe($open);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // The box is down to its title bar: no bottom border follows it.
    expect($frame)->toContain('┌─ RECORD  (4)')
        ->and($browser->document->section(RowDocument::RECORD))->toBe([]);
});

it('floats over the grid rather than taking the screen', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // The panes are still there behind it.
    expect($frame)->toContain('TABLES')
        ->and($frame)->toContain('┌─ RECORD')
        ->and($frame)->toContain('events');
});

it('keeps the inspector inside the terminal at any size', function () {
    foreach ([[100, 24], [140, 30], [90, 20], [200, 50]] as [$cols, $rows]) {
        putenv("COLUMNS={$cols}");
        putenv("LINES={$rows}");

        $browser = inspectable();
        $browser->emit('key', 'i');

        $render = new ReflectionMethod($browser, 'renderTheme');
        $render->setAccessible(true);

        $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)));

        expect(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual($cols)
            ->and(count($lines))->toBeLessThanOrEqual($rows);
    }

    putenv('COLUMNS');
    putenv('LINES');
});

it('still shows one value on shift+i', function () {
    $browser = inspectable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'I');

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellEditor?->buffer())->toBe('user.signed_up')
        ->and($browser->cellColumn())->toBe('name');
});

it('selects and yanks lines', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'j');
    $browser->emit('key', 'V');
    $browser->emit('key', 'j');

    expect($browser->documentSelection())->toBe([1, 2]);

    $browser->emit('key', 'y');

    expect($browser->status)->toContain('yanked 2 lines')
        ->and($browser->documentAnchor)->toBeNull();
});

it('clears a selection with escape before closing', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'V');
    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('inspect')
        ->and($browser->documentAnchor)->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse');
});

it('edits the column the cursor is on with e', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    // record heading, then id, then name.
    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellColumn())->toBe('name')
        ->and($browser->cellEditor?->buffer())->toBe('user.signed_up');
});

it('says so when there is no row to inspect', function () {
    $browser = inspectable();

    $browser->raw = [];
    $browser->rows = [];

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('browse')
        ->and($browser->status)->toContain('no rows');
});

it('does not dim the values you opened it to read', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $line = collect(explode("\n", $render->invoke($browser)))
        ->first(fn (string $l) => str_contains($l, 'user.signed_up'));

    expect($line)->not->toBeNull()
        // The value carries no dim of its own; the pane chrome may.
        ->and($line)->not->toContain("\e[2muser.signed_up");
});

it('widens to fit a related collection instead of truncating it', function () {
    putenv('COLUMNS=160');
    putenv('LINES=40');

    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)));

    putenv('COLUMNS');
    putenv('LINES');

    // Nothing inside the modal is cut short.
    foreach ($lines as $line) {
        if (str_contains($line, 'user.signed_up')) {
            expect($line)->not->toContain('…');
        }
    }

    expect(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual(160);
});

it('says what kind of relation each one is', function () {
    $browser = relatedBrowser(<<<'SQL'
        create table artists (id integer primary key, name text);
        create table albums (id integer primary key, artist_id integer references artists(id), title text);
        create table passports (id integer primary key, artist_id integer unique references artists(id), number text);
        insert into artists (name) values ('AC/DC');
        insert into albums (artist_id, title) values (1, 'Let There Be Rock'), (1, 'Powerage');
        insert into passports (artist_id, number) values (1, 'X1');
    SQL, 'artists');

    $text = implode("\n", inspectorLines($browser));

    // A unique key on the other side is what makes it one rather than many.
    expect($text)->toContain('albums  ·  has many  (2)')
        ->and($text)->toContain('passports  ·  has one');
});

it('shows what is on the far side of a pivot', function () {
    $browser = relatedBrowser(<<<'SQL'
        create table films (id integer primary key, title text);
        create table actors (id integer primary key, name text);
        create table film_actor (film_id integer references films(id), actor_id integer references actors(id), last_update text);
        insert into films (title) values ('Airplane Sierra');
        insert into actors (name) values ('Penelope Guiness'), ('Christian Gable');
        insert into film_actor values (1, 1, 'now'), (1, 2, 'now');
    SQL, 'films');

    $text = implode("\n", inspectorLines($browser));

    // The actors, not two rows of timestamps.
    expect($text)->toContain('actors  ·  has many through film_actor  (2)')
        ->and($text)->toContain('Penelope Guiness')
        ->and($text)->not->toContain('film_actor  ·  has many');
});

it('leaves a join table alone when it carries data of its own', function () {
    $browser = relatedBrowser(<<<'SQL'
        create table orders (id integer primary key, reference text);
        create table products (id integer primary key, name text);
        create table order_lines (id integer primary key, order_id integer references orders(id), product_id integer references products(id), quantity integer);
        insert into orders (reference) values ('A-1');
        insert into products (name) values ('Widget');
        insert into order_lines (order_id, product_id, quantity) values (1, 1, 7);
    SQL, 'orders');

    // quantity is the whole point of that row, so it is not a pivot.
    expect(implode("\n", inspectorLines($browser)))->toContain('order_lines  ·  has many  (1)');
});

it('clamps a long column instead of dropping it', function () {
    $browser = relatedBrowser(<<<'SQL'
        create table authors (id integer primary key, name text);
        create table books (
            id integer primary key,
            author_id integer references authors(id),
            title text,
            blurb text,
            isbn text
        );
        insert into authors (name) values ('Ursula');
        insert into books (author_id, title, blurb, isbn)
            values (1, 'The Dispossessed', 'A very long description of the book that would push every other column off the side of the screen', '978');
    SQL, 'authors');

    $text = implode("\n", inspectorLines($browser));

    // Every column is there; the prose is cut with an ellipsis, and i on the
    // row in the table opens the whole value.
    expect($text)->toContain('blurb')
        ->and($text)->toContain('isbn')
        ->and($text)->toContain('The Dispossessed')
        ->and($text)->toContain('…')
        ->and($text)->not->toContain('off the side of the screen')
        ->and($text)->not->toContain('columns');
});

it('leaves a narrow related row alone', function () {
    $browser = relatedBrowser(<<<'SQL'
        create table teams (id integer primary key, name text);
        create table players (id integer primary key, team_id integer references teams(id), name text, number integer);
        insert into teams (name) values ('Rovers');
        insert into players (team_id, name, number) values (1, 'Ada', 9);
    SQL, 'teams');

    $text = implode("\n", inspectorLines($browser));

    expect($text)->toContain('players  ·  has many  (1)')
        ->and($text)->toContain('Ada')
        ->and($text)->not->toContain('…');
});

it('lays a related row out to the width it has', function () {
    $sql = <<<'SQL'
        create table authors (id integer primary key, name text);
        create table books (id integer primary key, author_id integer references authors(id), title text, blurb text);
        insert into authors (name) values ('Ursula');
        insert into books (author_id, title, blurb)
            values (1, 'The Dispossessed', 'A very long description that would run off the side of a narrow terminal but not a wide one');
    SQL;

    $blurb = function (int $columns) use ($sql) {
        putenv("COLUMNS={$columns}");
        putenv('LINES=40');

        $browser = relatedBrowser($sql, 'authors');

        $text = implode("\n", inspectorLines($browser));

        putenv('COLUMNS');
        putenv('LINES');

        preg_match('/A very long[^\n]*/', $text, $match);

        return mb_strlen($match[0] ?? '');
    };

    // The same row, laid out to two different terminals.
    expect($blurb(200))->toBeGreaterThan($blurb(100));
});
