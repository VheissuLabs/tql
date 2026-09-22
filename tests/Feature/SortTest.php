<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\QueryEditor;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0]);
});

function sortable(): Browser
{
    $path = sys_get_temp_dir().'/tql-sort-'.uniqid().'.sqlite';
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

it('sorts ascending then descending then back to the primary key', function () {
    $browser = sortable();

    expect($browser->sortColumn)->toBe('id');

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBe('name')
        ->and($browser->sortDirection)->toBe('asc')
        ->and(array_column($browser->rows, 'name'))->toBe(['apple', 'banana', 'cherry']);

    $browser->sortBy('name');

    expect($browser->sortDirection)->toBe('desc')
        ->and(array_column($browser->rows, 'name'))->toBe(['cherry', 'banana', 'apple']);

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBe('id')
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

    expect($browser->lastStatement)->toContain('order by')
        ->and($browser->lastStatement)->toContain('"id" asc');
});

it('marks the sorted column in the header', function () {
    $browser = sortable();

    expect(rerender($browser))->toContain('id ▲');

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

it('goes back to the primary key when you change table', function () {
    $browser = sortable();
    $browser->sortBy('name');

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->sortColumn)->toBe('id');
});

it('sorts query results by rewriting the statement it shows', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit');
    $browser->emit('key', QueryEditor::RUN);

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBe('name')
        ->and($browser->sortDirection)->toBe('asc')
        ->and($browser->editor->buffer())->toBe('select * from fruit order by "name" asc')
        ->and(array_column($browser->raw, 'name'))->toBe(['apple', 'banana', 'cherry']);

    $browser->sortBy('name');

    expect($browser->sortDirection)->toBe('desc')
        ->and($browser->editor->buffer())->toBe('select * from fruit order by "name" desc')
        ->and(array_column($browser->raw, 'name'))->toBe(['cherry', 'banana', 'apple']);

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBeNull()
        ->and($browser->editor->buffer())->toBe('select * from fruit');
});

it('keeps a where clause when it sorts a query', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit where qty > 1');
    $browser->emit('key', QueryEditor::RUN);

    $browser->sortBy('qty');

    expect($browser->editor->buffer())
        ->toBe('select * from fruit where qty > 1 order by "qty" asc')
        ->and(array_column($browser->raw, 'name'))->toBe(['cherry', 'apple']);
});

it('says so rather than mangling a query it cannot sort', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select name from fruit union select name from veg');
    $browser->emit('key', QueryEditor::RUN);

    $browser->sortBy('name');

    expect($browser->sortColumn)->toBe('id')
        ->and($browser->editor->buffer())->toBe('select name from fruit union select name from veg')
        ->and($browser->status)->toContain('too complex to sort');
});

it('focuses the sql pane when it is clicked', function () {
    config(['tql.ui.sql_always' => true, 'tql.ui.sql_position' => 'bottom']);

    $browser = sortable();
    $browser->editor->set("select *\nfrom fruit");

    rerender($browser);

    expect($browser->mode)->toBe('browse');

    $island = $browser->editorIsland;

    expect($island)->not->toBeNull();

    $browser->emit('key', sprintf("\e[<0;%d;%dM", $island->x + 4, $island->y + 2));

    expect($browser->mode)->toBe('query')
        ->and($browser->editor->cursorLine())->toBe(1)
        ->and($browser->editor->cursorColumn())->toBe(3);
});

it('focuses the sql pane from a click on its border', function () {
    config(['tql.ui.sql_always' => true, 'tql.ui.sql_position' => 'bottom']);

    $browser = sortable();

    rerender($browser);

    $island = $browser->editorIsland;

    $browser->emit('key', sprintf("\e[<0;%d;%dM", $island->x, $island->y));

    expect($browser->mode)->toBe('query');
});

it('sorts by the primary key before you touch anything', function () {
    $browser = sortable();

    expect($browser->sortColumn)->toBe('id')
        ->and($browser->sortDirection)->toBe('asc')
        ->and($browser->lastStatement)->toContain('order by "id" asc')
        ->and(rerender($browser))->toContain('id ▲');
});

it('leaves the sort alone on a table with no primary key', function () {
    $browser = sortable();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->sortColumn)->toBe('id');
});

it('keeps the sort marker rather than truncating it away', function () {
    $browser = sortable();

    expect(rerender($browser))->toContain('id ▲')
        ->and(rerender($browser))->not->toContain('id…');
});

it('sorts the column under the cursor with o and cycles it', function () {
    $browser = sortable();

    $browser->emit('key', 'l');

    expect($browser->headers[$browser->columnIndex])->toBe('name');

    $browser->emit('key', 'o');

    expect($browser->sortColumn)->toBe('name')
        ->and($browser->sortDirection)->toBe('asc');

    $browser->emit('key', 'o');

    expect($browser->sortDirection)->toBe('desc');

    $browser->emit('key', 'o');

    expect($browser->sortColumn)->toBe('id');
});

it('offers the sort key on the hotkey bar', function () {
    expect(rerender(sortable()))->toContain('o Sort');
});

it('leaves the cursor on the column it sorted', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'l');

    expect($browser->headers[$browser->columnIndex])->toBe('qty');

    $browser->emit('key', 'o');

    expect($browser->headers[$browser->columnIndex])->toBe('qty')
        ->and($browser->sortColumn)->toBe('qty');
});

it('keeps the cursor row across a sort', function () {
    $browser = sortable();

    $browser->emit('key', 'j');
    $browser->emit('key', 'o');

    expect($browser->rowIndex)->toBe(1);
});

it('still jumps to the first cell when you change table', function () {
    $browser = sortable();

    $browser->emit('key', 'j');
    $browser->emit('key', 'l');

    expect($browser->rowIndex)->toBe(1)
        ->and($browser->columnIndex)->toBe(1);

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->rowIndex)->toBe(0)
        ->and($browser->columnIndex)->toBe(0);
});

it('resizes a column with the unshifted keys', function () {
    $browser = sortable();

    $browser->emit('key', 'l');

    $name = $browser->headers[$browser->columnIndex];

    $browser->emit('key', '.');
    $browser->emit('key', '.');

    $wider = $browser->widthOverrides[$name];

    $browser->emit('key', ',');

    expect($wider)->toBeGreaterThan($browser->widthOverrides[$name]);
});

it('still resizes with the shifted keys', function () {
    $browser = sortable();

    $browser->emit('key', 'l');

    $name = $browser->headers[$browser->columnIndex];

    $browser->emit('key', '>');

    expect($browser->widthOverrides)->toHaveKey($name);

    $wider = $browser->widthOverrides[$name];

    $browser->emit('key', '<');

    expect($browser->widthOverrides[$name])->toBeLessThan($wider);
});

it('accepts a pasted query in the sql editor', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('');

    $browser->emit('key', "select *\nfrom fruit\nwhere qty > 1");

    expect($browser->editor->buffer())->toBe("select *\nfrom fruit\nwhere qty > 1");
});

it('accepts a pasted path on the command line', function () {
    $browser = sortable();

    $browser->emit('key', ':');
    $browser->emit('key', 'export ~/Code/tql/out.sql');

    expect($browser->command)->toBe('export ~/Code/tql/out.sql');
});

it('marks a row for deletion without writing it', function () {
    $browser = sortable();

    expect($browser->pendingDeletes)->toBe([]);

    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(1)
        ->and($browser->rows)->toHaveCount(3)
        ->and($browser->status)->toContain('1 row marked for deletion');

    // Still in the database.
    expect(array_column($browser->raw, 'name'))->toHaveCount(3);
});

it('moves down after marking so you can mark a run of rows', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(2)
        ->and($browser->markedRows())->toBe([0, 1]);
});

it('unmarks a row you mark twice', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'k');
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toBe([]);
});

it('clears every mark with u', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'd');
    $browser->emit('key', 'u');

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->status)->toBe('marks cleared');
});

it('writes the marked rows on :w', function () {
    $browser = sortable();

    $browser->emit('key', 'd');

    $gone = $browser->raw[0]['name'];

    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->status)->toContain('deleted 1 row')
        ->and(array_column($browser->raw, 'name'))->not->toContain($gone)
        ->and($browser->raw)->toHaveCount(2);
});

it('says so when there is nothing to write', function () {
    $browser = sortable();

    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");

    expect($browser->status)->toBe('nothing to write');
});

it('keeps a mark on the right row when the sort changes', function () {
    $browser = sortable();

    $browser->emit('key', 'd');

    $marked = $browser->pendingDeletes[0];

    $browser->sortBy('name');

    expect($browser->pendingDeletes)->toBe([$marked])
        ->and($browser->markedRows())->toHaveCount(1);
});

it('will not mark a row in query results', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);
    $browser->emit('key', "\e");

    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->status)->toContain('no row to delete');
});

it('drops unwritten marks before quitting rather than losing them silently', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'q');

    expect($browser->state)->not->toBe('submit')
        ->and($browser->pendingDeletes)->toBe([])
        ->and($browser->status)->toContain('dropped');

    $browser->emit('key', 'q');

    expect($browser->state)->toBe('submit');
});
