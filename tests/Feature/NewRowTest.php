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

function adding(?string $create = null, array $attributes = []): Browser
{
    $path = sys_get_temp_dir().'/tql-add-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec($create ?? 'create table widgets (id integer primary key, name text, qty integer default 1)');
    $pdo->exec("insert into widgets (name, qty) values ('alpha', 3)");

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

    $added = $browser->rows[$browser->firstAddedRow()];

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
        ->and($browser->rows[$browser->firstAddedRow()]['name'])->toBe('beta');
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
