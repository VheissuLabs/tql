<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config([
        'tql.ui.mouse_row_offset' => 0,
        'tql.ui.mouse_column_offset' => 0,
        'tql.ui.sql_always' => true,
        'tql.ui.sql_position' => 'bottom',
    ]);
});

function clickablePanes(): Browser
{
    $path = sys_get_temp_dir().'/tql-panes-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table fruit (id integer primary key, name text)');
    $pdo->exec('create table veg (id integer primary key, name text)');
    $pdo->exec("insert into fruit (name) values ('cherry'), ('apple')");

    $connection = Connection::create([
        'name' => 'panes'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    redrawPanes($browser);
    $browser->emit('key', "\n");
    redrawPanes($browser);

    return $browser;
}

function redrawPanes(Browser $browser): void
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);
}

function clickAt(Browser $browser, int $column, int $row): void
{
    $browser->emit('key', sprintf("\e[<0;%d;%dM", $column, $row));
    redrawPanes($browser);
}

function clickSidebarTable(Browser $browser, string $table): void
{
    $sidebar = $browser->sidebar;
    $index = array_search($table, $browser->tables, true);

    clickAt($browser, $sidebar->x + 2, $sidebar->y + 1 + ($index - $sidebar->start));
}

function clickFirstRow(Browser $browser): void
{
    clickAt($browser, $browser->table->x + 3, $browser->table->y + 3);
}

it('focuses the table list when a table in it is clicked', function () {
    $browser = clickablePanes();

    expect($browser->focus)->toBe('grid');

    clickSidebarTable($browser, 'veg');

    expect($browser->focus)->toBe('sidebar')
        ->and($browser->tables[$browser->tableIndex])->toBe('veg');
});

it('focuses the table list from a click on its border', function () {
    $browser = clickablePanes();
    $before = $browser->tableIndex;

    clickAt($browser, $browser->sidebar->x, $browser->sidebar->y);

    expect($browser->focus)->toBe('sidebar')
        ->and($browser->tableIndex)->toBe($before);
});

it('focuses the rows from a click on their border', function () {
    $browser = clickablePanes();
    $browser->emit('key', "\e1");

    expect($browser->focus)->toBe('sidebar');

    clickAt($browser, $browser->table->x, $browser->table->y);

    expect($browser->focus)->toBe('grid');
});

it('leaves the sql editor for the rows when a row is clicked', function () {
    $browser = clickablePanes();
    $browser->emit('key', 's');
    redrawPanes($browser);

    expect($browser->mode)->toBe('query');

    clickFirstRow($browser);

    expect($browser->mode)->toBe('browse')
        ->and($browser->focus)->toBe('grid');
});

it('leaves the sql editor for the table list when a table is clicked', function () {
    $browser = clickablePanes();
    $browser->emit('key', 's');
    redrawPanes($browser);

    clickSidebarTable($browser, 'veg');

    expect($browser->mode)->toBe('browse')
        ->and($browser->focus)->toBe('sidebar')
        ->and($browser->tables[$browser->tableIndex])->toBe('veg');
});

it('leaves the sql editor when a column header is clicked', function () {
    $browser = clickablePanes();
    $browser->emit('key', 's');
    redrawPanes($browser);

    clickAt($browser, $browser->table->x + 3, $browser->table->y + 1);

    expect($browser->mode)->toBe('browse')
        ->and($browser->focus)->toBe('grid');
});
