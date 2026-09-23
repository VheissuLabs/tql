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

function adding(?string $create = null, array $attributes = [], ?string $seed = null): Browser
{
    $path = sys_get_temp_dir().'/tql-add-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec($create ?? 'create table widgets (id integer primary key, name text, qty integer default 1)');
    $pdo->exec($seed ?? "insert into widgets (name, qty) values ('alpha', 3)");

    $connection = Connection::create(array_merge([
        'name' => 'add'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ], $attributes));

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

function typeInto(Browser $browser, string $text): void
{
    foreach (mb_str_split($text) as $character) {
        $browser->emit('key', $character);
    }
}

function write(Browser $browser): void
{
    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");
}

function rowsOf(Browser $browser): array
{
    return (new PDO('sqlite:'.$browser->connection->database))
        ->query('select * from widgets')
        ->fetchAll(PDO::FETCH_ASSOC);
}

it('adds a row, fills it in and writes it', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    expect($browser->pendingInserts)->toHaveCount(1)
        ->and($browser->onAddedRow())->toBeTrue()
        // On top, where you are already looking.
        ->and($browser->rowIndex)->toBe(0)
        // It starts on the first column the database is not filling in itself.
        ->and($browser->headers[$browser->columnIndex])->toBe('name');

    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    write($browser);

    $rows = rowsOf($browser);

    expect($rows)->toHaveCount(2)
        ->and($rows[1]['name'])->toBe('beta')
        // qty was never touched, so the column default applied.
        ->and($rows[1]['qty'])->toBe(1)
        ->and($browser->pendingInserts)->toBe([])
        ->and($browser->status)->toContain('1 row added');
});

it('shows an untouched column as blank rather than NULL', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    $added = $browser->rows[0];

    expect($added['qty'])->toBe('')
        ->and($added['id'])->toBe('');
});

it('drops a row that was never written', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    $browser->emit('key', 'u');

    expect($browser->pendingInserts)->toBe([])
        ->and($browser->rows)->toHaveCount(1)
        ->and(rowsOf($browser))->toHaveCount(1);
});

it('keeps an unwritten row through a reload', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    $browser->emit('key', 'r');

    expect($browser->pendingInserts)->toHaveCount(1)
        ->and($browser->rows[0]['name'])->toBe('beta');
});

it('refuses on a read-only connection', function () {
    $browser = adding(attributes: ['read_only' => true]);

    $browser->emit('key', 'N');

    expect($browser->pendingInserts)->toBe([])
        ->and($browser->status)->toContain('read-only');
});

it('refuses on query results, which have no table to add to', function () {
    $browser = adding();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', "\n");

    $browser->emit('key', "\e");
    $browser->emit('key', 'N');

    expect($browser->pendingInserts)->toBe([])
        ->and($browser->status)->toContain('no table to add to');
});

it('adds to a table with no primary key', function () {
    $browser = adding('create table widgets (name text, qty integer default 1)');

    $browser->emit('key', 'N');
    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    write($browser);

    expect(rowsOf($browser))->toHaveCount(2)
        ->and($browser->status)->toContain('1 row added');
});

it('puts the newest row on top', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    $browser->emit('key', 'e');
    typeInto($browser, 'first');
    $browser->emit('key', "\n");

    $browser->emit('key', 'N');
    $browser->emit('key', 'e');
    typeInto($browser, 'second');
    $browser->emit('key', "\n");

    expect($browser->rows[0]['name'])->toBe('second')
        ->and($browser->rows[1]['name'])->toBe('first')
        ->and($browser->rows[2]['name'])->toBe('alpha')
        ->and($browser->addedRows())->toBe([0, 1]);

    write($browser);

    expect(array_column(rowsOf($browser), 'name'))->toBe(['alpha', 'second', 'first']);
});

it('leaves an auto-increment key to the database', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    expect($browser->requiredColumns())->not->toContain('id')
        ->and($browser->pendingInserts[0])->toBe([])
        ->and($browser->headers[$browser->columnIndex])->toBe('name');
});

it('fills a key the database will not give out, and says which are left', function () {
    // A smallint primary key is not SQLite's rowid, so nothing fills it in.
    $browser = adding(
        'create table widgets (id smallint not null primary key, name text not null, qty integer default 1)',
        seed: "insert into widgets (id, name, qty) values (1, 'alpha', 3)",
    );

    $browser->emit('key', 'N');

    expect($browser->requiredColumns())->toContain('name')
        ->and($browser->pendingInserts[0]['id'])->toBe(2)
        // Past the one it filled in, onto the one you have to.
        ->and($browser->headers[$browser->columnIndex])->toBe('name')
        ->and($browser->status)->toContain('id 2')
        ->and($browser->status)->toContain('name to fill in');

    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    write($browser);

    expect(rowsOf($browser))->toHaveCount(2)
        ->and($browser->problem)->toBeNull();
});

it('says what must be filled in when nothing can be guessed', function () {
    $browser = adding(
        'create table widgets (code text not null primary key, name text not null)',
        seed: "insert into widgets (code, name) values ('a', 'alpha')",
    );

    $browser->emit('key', 'N');

    expect($browser->requiredColumns())->toBe(['code', 'name'])
        // A text key is not a number, so there is no next one to offer.
        ->and($browser->pendingInserts[0])->toBe([])
        ->and($browser->status)->toContain('code, name to fill in');
});

it('shows which column you are on inside a pending row', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $rowOf = function (string $frame, string $needle) {
        foreach (explode("\n", $frame) as $line) {
            if (str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), $needle)) {
                return $line;
            }
        }

        return '';
    };

    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    $row = $rowOf($render->invoke($browser), 'beta');

    // The bar is broken into spans so the cell under the cursor is its own,
    // rather than one flat colour with nothing to say where you are.
    expect(substr_count($row, "\e[7m"))->toBeGreaterThan(1);

    // Off the grid, the row is one colour again: the cursor belongs to the
    // pane you are in.
    $browser->focus = 'sidebar';

    $row = $rowOf($render->invoke($browser), 'beta');

    expect(substr_count($row, "\e[7m"))->toBe(1);
});
