<?php

use App\Database\QueryRunner;
use App\Keys\Keymap;
use App\Keys\Keys;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false, 'tql.keys' => []]);
    Keymap::forget();
});

function counted(): Browser
{
    $path = sys_get_temp_dir().'/tql-count-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table a_table (id integer primary key, b text, c text, d text, e text, f text)');

    foreach (range(1, 20) as $n) {
        $pdo->exec("insert into a_table (b) values ('row {$n}')");
    }

    $pdo->exec('create table b_table (id integer primary key)');
    $pdo->exec('create table c_table (id integer primary key)');
    $pdo->exec('create table d_table (id integer primary key)');

    $connection = Connection::create(['name' => 'count'.uniqid(), 'driver' => 'sqlite', 'database' => $path]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");

    return $browser;
}

function keys(Browser $browser, string ...$keys): void
{
    foreach ($keys as $key) {
        $browser->emit('key', $key);
    }
}

it('moves across as many columns as the count says', function () {
    $browser = counted();

    keys($browser, '3', 'l');

    expect($browser->columnIndex)->toBe(3);

    keys($browser, '2', 'h');

    expect($browser->columnIndex)->toBe(1);
});

it('moves down and up as many rows as the count says', function () {
    $browser = counted();

    keys($browser, '5', 'j');
    expect($browser->rowIndex)->toBe(5);

    keys($browser, '1', '2', 'j');
    expect($browser->rowIndex)->toBe(17);

    keys($browser, '1', '0', '0', 'j');
    expect($browser->rowIndex)->toBe(19);

    keys($browser, '4', 'k');
    expect($browser->rowIndex)->toBe(15);
});

it('stops at the first column rather than carrying on into the table list', function () {
    $browser = counted();

    keys($browser, 'l', '5', 'h');

    expect($browser->columnIndex)->toBe(0)
        ->and($browser->focus)->toBe('grid');

    keys($browser, 'h');

    expect($browser->focus)->toBe('sidebar');
});

it('moves through the table list by a count', function () {
    $browser = counted();

    keys($browser, "\e1", '2', 'j');

    expect($browser->currentTable())->toBe('c_table');
});

it('does not treat a leading 0 as a count, and forgets a count on any other key', function () {
    $browser = counted();

    keys($browser, '0', 'j');
    expect($browser->rowIndex)->toBe(1);

    keys($browser, '3', 'r', 'j');
    expect($browser->rowIndex)->toBe(2);
});

it('switches panes with alt and a number, leaving the digits for counts', function () {
    $browser = counted();

    keys($browser, "\e1");
    expect($browser->focus)->toBe('sidebar');

    keys($browser, "\e2");
    expect($browser->focus)->toBe('grid');

    keys($browser, "\e3");
    expect($browser->mode)->toBe('query');
});

it('reads and writes alt keys the way a person would', function () {
    expect(Keys::bytes('alt+1'))->toBe("\e1")
        ->and(Keys::bytes('option+x'))->toBe("\ex")
        ->and(Keys::spell("\e2"))->toBe('alt+2')
        ->and(Keys::glyph("\e3"))->toBe(PHP_OS_FAMILY === 'Darwin' ? '⌥3' : 'alt+3')
        ->and(Keymap::key('focus_rows'))->toBe('alt+2');
});
