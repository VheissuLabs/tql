<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\QueryEditor;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['dotsql.ui.mouse_row_offset' => 0]);
});

function sortable(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-sort-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table fruit (id integer primary key, name text, qty integer)');
    $pdo->exec('create table veg (id integer primary key, name text)');

    $insert = $pdo->prepare('insert into fruit (name, qty) values (?, ?)');
    $insert->execute(['cherry', 3]);
    $insert->execute(['apple', 9]);
    $insert->execute(['banana', 1]);

    $connection = Connection::create([
        'name' => 'sort'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $render->invoke($browser);

    return $browser;
}

function rerender(Browser $browser): string
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser));
}

it('sorts ascending then descending then clears', function () {
    $browser = sortable();

    expect($browser->sortColumn)->toBeNull();

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBe('name')
        ->and($browser->sortDirection)->toBe('asc')
        ->and(array_column($browser->rows, 'name'))->toBe(['apple', 'banana', 'cherry']);

    $browser->sortBy('name');

    expect($browser->sortDirection)->toBe('desc')
        ->and(array_column($browser->rows, 'name'))->toBe(['cherry', 'banana', 'apple']);

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBeNull()
        ->and(array_column($browser->rows, 'name'))->toBe(['cherry', 'apple', 'banana']);
});

it('switches to another column rather than toggling', function () {
    $browser = sortable();

    $browser->sortBy('name');
    $browser->sortBy('qty');

    expect($browser->sortColumn)->toBe('qty')
        ->and($browser->sortDirection)->toBe('asc')
        ->and(array_column($browser->rows, 'qty'))->toBe(['1', '3', '9']);
});

it('puts the order by into the query it shows', function () {
    $browser = sortable();

    $browser->sortBy('qty');

    expect($browser->lastStatement)->toContain('order by')
        ->and($browser->lastStatement)->toContain('qty')
        ->and($browser->lastStatement)->toContain('asc');

    $browser->sortBy('qty');

    expect($browser->lastStatement)->toContain('desc');

    $browser->sortBy('qty');

    expect($browser->lastStatement)->not->toContain('order by');
});

it('marks the sorted column in the header', function () {
    $browser = sortable();

    expect(rerender($browser))->not->toContain('▲');

    $browser->sortBy('name');

    expect(rerender($browser))->toContain('name ▲');

    $browser->sortBy('name');

    expect(rerender($browser))->toContain('name ▼');
});

it('sorts by clicking the column header', function () {
    $browser = sortable();
    rerender($browser);

    $row = $browser->table->y + 1;
    $column = $browser->table->cellStart(1) + 2;

    $browser->emit('key', "\e[<0;{$column};{$row}M");

    expect($browser->sortColumn)->toBe('name')
        ->and($browser->sortDirection)->toBe('asc');

    $browser->emit('key', "\e[<0;{$column};{$row}M");

    expect($browser->sortDirection)->toBe('desc');
});

it('sorts the focused column with o', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'o');

    expect($browser->sortColumn)->toBe('name');
});

it('goes back to the first page when the sort changes', function () {
    $browser = sortable();
    $browser->emit('key', 'n');

    $browser->sortBy('name');

    expect($browser->offset)->toBe(0);
});

it('forgets the sort when you change table', function () {
    $browser = sortable();
    $browser->sortBy('name');

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->sortColumn)->toBeNull();
});

it('does not sort query results', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);

    $browser->sortBy('one');

    expect($browser->sortColumn)->toBeNull();
});
