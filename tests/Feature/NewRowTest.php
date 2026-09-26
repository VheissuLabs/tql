<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\QueryEditor;
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

function keepField(Browser $browser, string $text): void
{
    $browser->emit('key', "\n");
    typeInto($browser, $text);
    $browser->emit('key', "\n");
}

function blankRow(Browser $browser): void
{
    $browser->emit('key', 'N');
    $browser->emit('key', "\x13");
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

it('adds a row through the form and writes it', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    expect($browser->recordForm)->not->toBeNull()
        ->and($browser->recordForm->current()['name'])->toBe('name')
        ->and($browser->pendingInserts)->toBe([]);

    keepField($browser, 'beta');
    $browser->emit('key', "\x13");

    expect($browser->recordForm)->toBeNull()
        ->and($browser->pendingInserts)->toHaveCount(1)
        ->and($browser->onAddedRow())->toBeTrue()
        ->and($browser->rowIndex)->toBe(0);

    write($browser);

    $rows = rowsOf($browser);

    expect($rows)->toHaveCount(2)
        ->and($rows[1]['name'])->toBe('beta')
        ->and($rows[1]['qty'])->toBe(1)
        ->and($browser->pendingInserts)->toBe([])
        ->and($browser->status)->toContain('1 row added');
});

it('fills in a plain default so you can see it', function () {
    $browser = adding();

    $browser->emit('key', 'N');

    expect($browser->recordForm->value('qty'))->toBe('1')
        ->and($browser->recordForm->state('qty'))->toBe('value');
});

it('shows an untouched column as blank rather than NULL', function () {
    $browser = adding();

    blankRow($browser);

    expect($browser->rows[0]['id'])->toBe('')
        ->and($browser->rows[0]['name'])->toBe('');
});

it('writes an explicit NULL rather than the default', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    $browser->emit('key', 'j');
    $browser->emit('key', "\x0e");
    $browser->emit('key', "\x13");

    expect($browser->pendingInserts[0])->toHaveKey('qty')
        ->and($browser->pendingInserts[0]['qty'])->toBeNull()
        ->and($browser->rows[0]['qty'])->toBe('NULL');

    write($browser);

    expect(rowsOf($browser)[1]['qty'])->toBeNull();
});

it('drops a row that was never written', function () {
    $browser = adding();

    blankRow($browser);
    $browser->emit('key', 'u');

    expect($browser->pendingInserts)->toBe([])
        ->and($browser->rows)->toHaveCount(1)
        ->and(rowsOf($browser))->toHaveCount(1);
});

it('keeps an unwritten row through a reload', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    keepField($browser, 'beta');
    $browser->emit('key', "\x13");

    $browser->emit('key', 'r');

    expect($browser->pendingInserts)->toHaveCount(1)
        ->and($browser->rows[0]['name'])->toBe('beta');
});

it('refuses on a read-only connection', function () {
    $browser = adding(attributes: ['read_only' => true]);

    $browser->emit('key', 'N');

    expect($browser->recordForm)->toBeNull()
        ->and($browser->pendingInserts)->toBe([])
        ->and($browser->status)->toContain('read-only');
});

it('refuses on query results, which have no table to add to', function () {
    $browser = adding();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);

    $browser->emit('key', "\e");
    $browser->emit('key', 'N');

    expect($browser->recordForm)->toBeNull()
        ->and($browser->status)->toContain('no table to add to');
});

it('adds to a table with no primary key', function () {
    $browser = adding('create table widgets (name text, qty integer default 1)');

    $browser->emit('key', 'N');
    keepField($browser, 'beta');
    $browser->emit('key', "\x13");

    write($browser);

    expect(rowsOf($browser))->toHaveCount(2)
        ->and($browser->status)->toContain('1 row added');
});

it('puts the newest row on top', function () {
    $browser = adding();

    $browser->emit('key', 'N');
    keepField($browser, 'first');
    $browser->emit('key', "\x13");

    $browser->emit('key', 'N');
    keepField($browser, 'second');
    $browser->emit('key', "\x13");

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

    $form = $browser->recordForm;

    expect($browser->requiredColumns())->not->toContain('id')
        ->and($form->state('id'))->toBe('untouched')
        ->and($form->fields()[0]['hint'])->toBe('auto')
        ->and($form->current()['name'])->toBe('name');
});

it('fills a key the database will not give out, and marks what is left', function () {
    $browser = adding(
        'create table widgets (id smallint not null primary key, name text not null, qty integer default 1)',
        seed: "insert into widgets (id, name, qty) values (1, 'alpha', 3)",
    );

    $browser->emit('key', 'N');

    $form = $browser->recordForm;

    expect($browser->requiredColumns())->toContain('name')
        ->and($form->value('id'))->toBe('2')
        ->and($form->current()['name'])->toBe('name')
        ->and($form->fields()[1]['hint'])->toBe('required');

    keepField($browser, 'beta');
    $browser->emit('key', "\x13");

    write($browser);

    expect(rowsOf($browser))->toHaveCount(2)
        ->and($browser->problem)->toBeNull();
});

it('marks what must be filled in when nothing can be guessed', function () {
    $browser = adding(
        'create table widgets (code text not null primary key, name text not null)',
        seed: "insert into widgets (code, name) values ('a', 'alpha')",
    );

    $browser->emit('key', 'N');

    $form = $browser->recordForm;

    expect($browser->requiredColumns())->toBe(['code', 'name'])
        ->and($form->state('code'))->toBe('untouched')
        ->and(array_column($form->fields(), 'hint'))->toBe(['required', 'required']);
});

it('shows which column you are on inside a pending row', function () {
    $browser = adding();

    blankRow($browser);

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

    $browser->columnIndex = 1;
    $browser->emit('key', 'e');
    typeInto($browser, 'beta');
    $browser->emit('key', "\n");

    $row = $rowOf($render->invoke($browser), 'beta');

    expect(substr_count($row, "\e[7m"))->toBeGreaterThan(1);

    $browser->focus = 'sidebar';

    $row = $rowOf($render->invoke($browser), 'beta');

    expect(substr_count($row, "\e[7m"))->toBe(1);
});
