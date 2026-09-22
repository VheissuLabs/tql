<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\Layout;
use App\Tui\Mouse;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

function clickable(): Browser
{
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);

    $path = sys_get_temp_dir().'/tql-click-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text)');

    foreach (['alpha', 'beta', 'gamma'] as $name) {
        $pdo->prepare('insert into widgets (name) values (?)')->execute([$name]);
    }

    $connection = Connection::create([
        'name' => 'click'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
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
 * A click on the cell at the given row and column of the grid.
 */
function cellAt(Browser $browser, int $row, int $column): string
{
    $table = $browser->table;

    // Two rows of chrome: the header and the rule under it.
    $y = $table->y + 1 + 2 + $row;
    $x = ($table->cellStart($column) ?? $table->x) + 2;

    return sprintf("\e[<0;%d;%dM", $x, $y);
}

it('parses a button press', function () {
    expect(Mouse::parse("\e[<0;10;4M"))->toMatchArray([
        'button' => 0, 'column' => 10, 'row' => 4, 'pressed' => true,
    ]);
});

it('applies the configured coordinate offset', function () {
    config(['tql.ui.mouse_row_offset' => 1]);

    $event = Mouse::parse("\e[<0;10;4M");

    expect($event['row'])->toBe(3)
        ->and($event['raw_row'])->toBe(4);

    config(['tql.ui.mouse_row_offset' => 0]);

    expect(Mouse::parse("\e[<0;10;4M")['row'])->toBe(4);
});

it('parses a release as not pressed', function () {
    expect(Mouse::parse("\e[<0;10;4m")['pressed'])->toBeFalse();
});

it('parses wheel buttons', function () {
    expect(Mouse::parse("\e[<64;1;1M")['button'])->toBe(Mouse::WHEEL_UP)
        ->and(Mouse::parse("\e[<65;1;1M")['button'])->toBe(Mouse::WHEEL_DOWN);
});

it('handles coordinates beyond the legacy 223 limit', function () {
    expect(Mouse::parse("\e[<0;240;80M")['column'])->toBe(240);
});

it('ignores anything that is not a mouse sequence', function () {
    expect(Mouse::parse('q'))->toBeNull()
        ->and(Mouse::parse("\e[A"))->toBeNull();
});

it('maps columns to the correct pane', function () {
    [$from, $to] = Layout::sidebarColumns();
    $grid = Layout::gridFirstColumn();

    expect(Layout::inSidebar($from))->toBeTrue()
        ->and(Layout::inSidebar($to))->toBeTrue()
        ->and(Layout::inSidebar($to + 1))->toBeFalse()
        ->and(Layout::inGrid($grid))->toBeTrue()
        ->and(Layout::inGrid($from))->toBeFalse()
        ->and($grid)->toBeGreaterThan($to);
});

it('returns null for rows above the body', function () {
    $first = Layout::firstBodyRow(1);

    expect(Layout::bodyIndex($first - 1, $first))->toBeNull()
        ->and(Layout::bodyIndex($first, $first))->toBe(0)
        ->and(Layout::bodyIndex($first + 3, $first))->toBe(3);
});

it('shifts the body row with the frame padding', function () {
    expect(Layout::firstBodyRow(2) - Layout::firstBodyRow(0))->toBe(2)
        ->and(Layout::firstBodyRow(0))->toBe(Layout::TOP_BORDER_ROWS + 1);
});

it('opens the editor on a double click', function () {
    $browser = clickable();

    $cell = cellAt($browser, 1, 1);

    $browser->emit('key', $cell);

    expect($browser->mode)->toBe('browse')
        ->and($browser->rowIndex)->toBe(1);

    $browser->emit('key', $cell);

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellEditor)->not->toBeNull();
});

it('does not open the editor on two slow clicks', function () {
    config(['tql.ui.double_click_ms' => 0]);

    $browser = clickable();
    $cell = cellAt($browser, 1, 1);

    $browser->emit('key', $cell);
    $browser->emit('key', $cell);

    expect($browser->mode)->toBe('browse');

    config(['tql.ui.double_click_ms' => 400]);
});

it('does not open the editor on two clicks in different cells', function () {
    $browser = clickable();

    $browser->emit('key', cellAt($browser, 1, 1));
    $browser->emit('key', cellAt($browser, 2, 1));

    expect($browser->mode)->toBe('browse')
        ->and($browser->rowIndex)->toBe(2);
});

it('treats three clicks as one double click, not two', function () {
    $browser = clickable();
    $cell = cellAt($browser, 1, 1);

    $browser->emit('key', $cell);
    $browser->emit('key', $cell);

    expect($browser->mode)->toBe('edit');

    $browser->emit('key', "\e");
    $browser->emit('key', $cell);

    expect($browser->mode)->toBe('browse');
});
