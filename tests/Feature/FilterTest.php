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

function filterable(): Browser
{
    $path = sys_get_temp_dir().'/tql-filter-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);

    foreach (['albums', 'artists', 'customers', 'invoices', 'invoice_items', 'tracks'] as $table) {
        $pdo->exec("create table {$table} (id integer primary key, name text)");
        $pdo->exec("insert into {$table} (name) values ('one')");
    }

    $connection = Connection::create([
        'name' => 'filter'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

function frame(Browser $browser): string
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser));
}

function type(Browser $browser, string $text): void
{
    foreach (preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) as $char) {
        $browser->emit('key', $char);
    }
}

it('filters the tables list as you type', function () {
    $browser = filterable();

    expect($browser->visibleTables())->toHaveCount(6);

    $browser->emit('key', '/');
    type($browser, 'inv');

    expect($browser->visibleTables())->toBe(['invoice_items', 'invoices']);
});

it('matches anywhere in the name, not just the start', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'item');

    expect($browser->visibleTables())->toBe(['invoice_items']);
});

it('ignores case', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'ALB');

    expect($browser->visibleTables())->toBe(['albums']);
});

it('shows the filter in the sidebar title and the status line', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'inv');

    $frame = frame($browser);

    expect($frame)->toMatch('/^┌─ \\[1\\] \\S+ \\/inv /m')
        ->and($frame)->toContain('/inv');
});

it('backspaces a character at a time', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'invo');

    expect($browser->visibleTables())->toHaveCount(2);

    $browser->emit('key', "\x7f");
    $browser->emit('key', "\x7f");
    $browser->emit('key', "\x7f");

    // artists, invoice_items, invoices are the only names containing an "i".
    expect($browser->filter)->toBe('i')
        ->and($browser->visibleTables())->toBe(['artists', 'invoice_items', 'invoices']);
});

it('keeps the filter on enter and drops it on escape', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'inv');
    $browser->emit('key', "\n");

    expect($browser->filtering)->toBeFalse()
        ->and($browser->filter)->toBe('inv')
        ->and($browser->visibleTables())->toHaveCount(2);

    $browser->emit('key', '/');
    $browser->emit('key', "\e");

    expect($browser->filter)->toBeNull()
        ->and($browser->visibleTables())->toHaveCount(6);
});

it('opens the table the filter is pointing at', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'tracks');
    $browser->emit('key', "\n");

    expect($browser->currentTable())->toBe('tracks');
});

it('keeps the cursor on a table that still matches', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'inv');

    // Two matches, so an index of 5 from the full list would be out of range.
    $browser->emit('key', "\n");

    expect($browser->tableIndex)->toBeLessThan(2)
        ->and($browser->currentTable())->toBeIn(['invoices', 'invoice_items']);
});

it('says so when nothing matches', function () {
    $browser = filterable();

    $browser->emit('key', '/');
    type($browser, 'zzzz');

    expect($browser->visibleTables())->toBe([])
        ->and(frame($browser))->toContain('nothing matches');
});

it('does not treat a slash in the sql editor as a filter', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = filterable();

    $browser->emit('key', 's');
    $browser->emit('key', '/');

    expect($browser->filtering)->toBeFalse()
        ->and($browser->editor->buffer())->toContain('/');

    config(['tql.ui.sql_always' => false]);
});
