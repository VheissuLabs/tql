<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['dotsql.ui.mouse_row_offset' => 0, 'dotsql.ui.sql_always' => false]);
});

function aligned(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-align-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, name text, payload text)');
    $pdo->exec("insert into events (name, payload) values ('user.signed_up', '{\"a\":1}')");
    $pdo->exec("insert into events (name, payload) values ('order.placed', '{\"b\":2}')");

    $connection = Connection::create([
        'name' => 'align'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

function widths(Browser $browser): array
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return collect(explode("\n", $method->invoke($browser)))
        ->map(fn (string $line) => mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', rtrim($line))))
        ->all();
}

it('draws every framed row to the same width', function (int $cols) {
    putenv("COLUMNS={$cols}");

    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    $framed = collect(explode("\n", $method->invoke($browser)))
        ->map(fn (string $line) => preg_replace('/\e\[[0-9;]*m/', '', rtrim($line)))
        ->filter(fn (string $line) => preg_match('/[┌│└├]/u', $line) === 1)
        ->map(fn (string $line) => mb_strlen($line))
        ->values()
        ->all();

    putenv('COLUMNS');

    expect($framed)->not->toBeEmpty()
        ->and(array_unique($framed))->toHaveCount(1);
})->with([80, 120, 160, 200, 240]);

it('runs the selection to the full inner width of its pane', function () {
    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);

    $frame = $method->invoke($browser);

    $selected = collect(explode("\n", $frame))
        ->first(fn (string $line) => str_contains($line, "\e[7m"));

    preg_match('/\e\[7m(.*?)\e\[27m/', $selected, $match);

    expect(mb_strlen($match[1]))->toBe($browser->sidebar->innerWidth());
});

it('keeps the editor caret visible on a line that fills the pane', function () {
    config(['dotsql.ui.sql_always' => true]);

    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', 's');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    $browser->editor->set(str_repeat('a', 400));
    $browser->editor->toEnd();

    expect($method->invoke($browser))->toContain("\e[7m");

    config(['dotsql.ui.sql_always' => false]);
});

it('pads the cursor block by a column either side', function () {
    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    preg_match_all('/\e\[7m(.*?)\e\[27m/', $method->invoke($browser), $matches);

    // The first is the selected table in the sidebar; the cell cursor is next.
    $cell = $matches[1][1];

    expect($cell)->toStartWith(' ')
        ->and($cell)->toEndWith(' ')
        ->and(trim($cell))->toBe('1');
});
