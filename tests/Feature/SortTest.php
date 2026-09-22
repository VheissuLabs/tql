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

function rerenderRaw(Browser $browser): string
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return $method->invoke($browser);
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

    // The query has no order by, so nothing is marked as sorted.
    expect($browser->sortColumn)->toBeNull()
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
        ->and($browser->status)->toContain('1 marked for deletion');

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
        ->and($browser->status)->toBe('pending changes dropped');
});

it('writes the marked rows on :w', function () {
    $browser = sortable();

    $browser->emit('key', 'd');

    $gone = $browser->raw[0]['name'];

    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->status)->toContain('1 deleted')
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

it('marks several rows in a row', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'd');
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(3)
        ->and($browser->markedRows())->toBe([0, 1, 2]);
});

it('marks rows apart from each other', function () {
    $browser = sortable();

    $browser->emit('key', 'd');
    $browser->emit('key', 'j');
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(2)
        ->and($browser->markedRows())->toBe([0, 2]);
});

it('marks the last row without unmarking it', function () {
    $browser = sortable();

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($browser->rowIndex)->toBe(2);

    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(1);

    // Nowhere left to advance to, so a second d toggles the same row off.
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toBe([]);
});

it('marks the row you are looking at, not the table under the sidebar cursor', function () {
    $browser = sortable();

    $browser->focus = 'sidebar';
    $table = $browser->currentTable();

    $browser->emit('key', 'd');

    expect($browser->focus)->toBe('grid')
        ->and($browser->currentTable())->toBe($table)
        ->and($browser->pendingDeletes)->toHaveCount(1);
});

it('forgets marks when you change table', function () {
    $browser = sortable();

    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(1);

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->currentTable())->toBe('veg');
});

it('highlights a marked row as one unbroken bar', function () {
    config(['tql.theme.deleted' => 'red']);

    $browser = sortable();
    $browser->emit('key', 'd');

    $marked = collect(explode("\n", rerenderRaw($browser)))
        ->first(fn (string $line) => str_contains($line, "\e[31m\e[7m"));

    expect($marked)->not->toBeNull();

    preg_match('/\e\[7m(.*?)\e\[27m/', $marked, $match);

    // Nothing inside the span resets the highlight, so the bar does not tear.
    expect($match[1] ?? '')->not->toContain("\e[")
        ->and($match[1] ?? '')->toContain('│');
});

it('no longer puts a dash in the gutter', function () {
    $browser = sortable();
    $browser->emit('key', 'd');

    expect(rerender($browser))->not->toContain(' -');
});

it('holds an edit until :w and shows it in yellow', function () {
    config(['tql.theme.edited' => 'yellow']);

    $browser = sortable();

    $browser->emit('key', 'e');
    $browser->emit('key', "\x7f");
    $browser->emit('key', 'X');
    $browser->emit('key', "\x04");

    expect($browser->pendingEdits)->toHaveCount(1)
        ->and($browser->editedRows())->toBe([0])
        ->and($browser->status)->toContain('1 row edited');

    // The grid shows what you typed, not what is still on disk.
    expect(rerender($browser))->toContain('X');

    $edited = collect(explode("\n", rerenderRaw($browser)))
        ->first(fn (string $line) => str_contains($line, "\e[33m\e[7m"));

    expect($edited)->not->toBeNull();
});

it('writes a pending edit on :w', function () {
    $browser = sortable();

    $before = $browser->raw[0]['id'];

    // Edit the name, not the primary key.
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', "\x7f");
    $browser->emit('key', 'Z');
    $browser->emit('key', "\x04");

    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");

    expect($browser->pendingEdits)->toBe([])
        ->and($browser->status)->toContain('1 row updated');

    $row = collect($browser->raw)->firstWhere('id', $before);

    expect($row['name'])->toBe('cherrZ');
});

it('drops a pending edit with u', function () {
    $browser = sortable();

    $was = $browser->raw[0]['name'];

    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', 'Q');
    $browser->emit('key', "\x04");

    expect($browser->pendingEdits)->toHaveCount(1);

    $browser->emit('key', 'u');

    expect($browser->pendingEdits)->toBe([])
        ->and($browser->rowsWithEdits()[0]['name'])->toBe($was);
});

it('counts edits and deletions together', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', 'Q');
    $browser->emit('key', "\x04");
    $browser->emit('key', 'h');
    $browser->emit('key', 'j');
    $browser->emit('key', 'd');

    expect($browser->status)->toContain('1 row edited')
        ->and($browser->status)->toContain('1 marked for deletion');
});

it('marks the column the query you ran orders by', function () {
    $browser = sortable();

    expect($browser->sortColumn)->toBe('id');

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit order by "qty" desc');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->sortColumn)->toBe('qty')
        ->and($browser->sortDirection)->toBe('desc');

    $browser->emit('key', "\e");

    expect(rerender($browser))->toContain('qty ▼')
        ->and(rerender($browser))->not->toContain('id ▲');
});

it('marks nothing when the query you ran has no order by', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->sortColumn)->toBeNull();

    $browser->emit('key', "\e");

    expect(rerender($browser))->not->toContain('▲');
});

it('yanks the value under the cursor with y', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'y');

    expect($browser->status)->toContain('yanked name');
});

it('yanks the row as an object with Y', function () {
    $browser = sortable();

    $browser->emit('key', 'Y');

    expect($browser->status)->toContain('yanked row');
});

it('runs the query on enter and adds a line on shift+enter', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit');
    $browser->editor->toEnd();

    $browser->emit('key', "\e[13;2u");
    $browser->emit('key', 'where qty > 1');

    expect($browser->editor->buffer())->toBe("select * from fruit\nwhere qty > 1")
        ->and($browser->resultsFromQuery)->toBeFalse();

    $browser->emit('key', "\n");

    expect($browser->resultsFromQuery)->toBeTrue()
        ->and(array_column($browser->raw, 'name'))->toBe(['cherry', 'apple']);
});

it('still runs the query on ctrl+r', function () {
    $browser = sortable();

    $browser->emit('key', 's');
    $browser->editor->set('select * from fruit where qty > 8');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->raw)->toHaveCount(1);
});

it('keeps a value edit on enter', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', '!');
    $browser->emit('key', "\n");

    expect($browser->mode)->toBe('browse')
        ->and($browser->pendingEdits)->toHaveCount(1);
});

it('adds a line to a value on shift+enter', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', "\e[13;2u");
    $browser->emit('key', 'second');

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellEditor->buffer())->toContain("\nsecond");
});

it('applies the filter on a second enter', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'f');
    $browser->emit('key', 'an');

    // First enter leaves the input.
    $browser->emit('key', "\n");

    expect($browser->filterForm)->not->toBeNull()
        ->and($browser->filterForm->editor)->toBeNull()
        ->and($browser->filters)->toBeNull();

    // Second enter runs it.
    $browser->emit('key', "\n");

    expect($browser->filterForm)->toBeNull()
        ->and(array_column($browser->raw, 'name'))->toBe(['banana']);
});

it('types straight back into the filter value', function () {
    $browser = sortable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'f');
    $browser->emit('key', 'an');
    $browser->emit('key', "\n");

    expect($browser->filterForm->editor)->toBeNull();

    $browser->emit('key', 'x');

    // Typing carries on from the value rather than replacing it.
    expect($browser->filterForm->editor)->not->toBeNull()
        ->and($browser->filterForm->editor->buffer())->toBe('anx');
});

it('hides the cell cursor when the grid is not focused', function () {
    $browser = sortable();

    expect($browser->focus)->toBe('grid')
        ->and(rerenderRaw($browser))->toContain("\e[7m");

    $browser->emit('key', "\t");

    expect($browser->focus)->toBe('sidebar');

    // The sidebar's own selection is the only highlight left in the grid area.
    $lines = collect(explode("\n", rerenderRaw($browser)))
        ->filter(fn (string $line) => str_contains($line, 'cherry') || str_contains($line, 'apple'));

    foreach ($lines as $line) {
        expect($line)->not->toContain("\e[7m");
    }
});

it('keeps the row marker when the grid is not focused', function () {
    $browser = sortable();

    $browser->emit('key', "\t");

    expect(rerender($browser))->toContain('▸');
});

it('keeps the tables list showing which table you are in', function () {
    $browser = sortable();

    expect($browser->focus)->toBe('grid');

    // The sidebar is not focused, but it still says which table this is.
    // The title row also mentions the table, so take the sidebar's own entry.
    $line = collect(explode("\n", rerenderRaw($browser)))
        ->first(fn (string $l) => str_contains($l, "\e[7m fruit"));

    expect($line)->not->toBeNull();
});
