<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;
use Laravel\Prompts\Output\BufferedConsoleOutput;
use Laravel\Prompts\Prompt;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['dotsql.ui.mouse_row_offset' => 0, 'dotsql.ui.sql_always' => false]);
});

function aligned(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-align-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, name text, payload text, created_at text)');

    foreach ([
        ['user.signed_up', '{"user":{"id":42,"email":"karl@notarydash.com","plan":"pro"}}'],
        ['order.placed', '{"order":{"id":"ord_8812","total":149.99,"currency":"usd"}}'],
        ['webhook.failed', '{"endpoint":"https://example.com/hooks/notarydash","status":500}'],
    ] as [$name, $payload]) {
        $pdo->prepare('insert into events (name, payload, created_at) values (?, ?, ?)')
            ->execute([$name, $payload, '2026-09-22T15:11:32+00:00']);
    }

    $connection = Connection::create([
        'name' => 'align'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

function captureOutput(): BufferedConsoleOutput
{
    $output = new BufferedConsoleOutput;

    Prompt::setOutput($output);

    return $output;
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

it('never renders taller than the terminal with the sql pane on', function (int $lines, bool $query) {
    config(['dotsql.ui.sql_always' => true]);
    putenv('COLUMNS=200');
    putenv("LINES={$lines}");

    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");

    if ($query) {
        $browser->emit('key', 's');
    }

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    $rows = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser)));

    putenv('COLUMNS');
    putenv('LINES');
    config(['dotsql.ui.sql_always' => false]);

    expect(count($rows))->toBeLessThanOrEqual($lines);
})->with([[24, false], [30, false], [33, false], [40, false], [24, true], [33, true], [60, true]]);

it('never renders wider than the terminal at any width', function () {
    config(['dotsql.ui.sql_always' => true]);

    $over = [];

    foreach (range(120, 220, 2) as $cols) {
        putenv("COLUMNS={$cols}");
        putenv('LINES=40');

        $browser = aligned();

        $method = new ReflectionMethod($browser, 'renderTheme');
        $method->setAccessible(true);
        $method->invoke($browser);
        $browser->emit('key', "\n");

        foreach (['browse', 'query'] as $state) {
            if ($state === 'query') {
                $browser->emit('key', 's');
            }

            foreach (explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser))) as $line) {
                if (mb_strlen($line) > $cols) {
                    $over[] = $cols.' '.$state.': '.mb_strlen($line).' > '.$cols.' :: '.trim($line);
                }
            }
        }
    }

    putenv('COLUMNS');
    putenv('LINES');
    config(['dotsql.ui.sql_always' => false]);

    expect($over)->toBe([]);
});

it('does not change height when the sql pane takes focus', function (int $lines) {
    config(['dotsql.ui.sql_always' => true, 'dotsql.ui.sql_position' => 'bottom', 'dotsql.ui.sql_height' => 0]);
    putenv('COLUMNS=135');
    putenv("LINES={$lines}");

    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);
    $browser->emit('key', "\n");

    $before = substr_count($method->invoke($browser), "\n");

    $browser->emit('key', 's');

    $after = substr_count($method->invoke($browser), "\n");

    putenv('COLUMNS');
    putenv('LINES');
    config(['dotsql.ui.sql_always' => false, 'dotsql.ui.sql_position' => 'top', 'dotsql.ui.sql_height' => 0]);

    expect($after)->toBe($before);
})->with([20, 24, 30, 33, 36, 40, 50]);

it('repaints the whole screen when the terminal is resized', function () {
    putenv('COLUMNS=120');
    putenv('LINES=30');

    $browser = aligned();
    $output = captureOutput();

    $render = new ReflectionMethod($browser, 'render');
    $render->setAccessible(true);
    $render->invoke($browser);

    $output->fetch();

    putenv('COLUMNS=160');
    putenv('LINES=40');

    $render->invoke($browser);

    $written = $output->fetch();

    putenv('COLUMNS');
    putenv('LINES');

    // The clear-and-home that stops the old frame's top row being orphaned.
    expect($written)->toContain("\e[2J\e[H");
});

it('redraws on ctrl+l', function () {
    $browser = aligned();
    $output = captureOutput();

    $render = new ReflectionMethod($browser, 'render');
    $render->setAccessible(true);
    $render->invoke($browser);

    $output->fetch();

    $browser->emit('key', "\x0c");
    $render->invoke($browser);

    expect($output->fetch())->toContain("\e[2J\e[H")
        ->and($browser->status)->toBe('redrawn');
});

it('clamps the hotkey bar and status line to the terminal width', function () {
    $over = [];

    foreach (range(60, 220, 2) as $cols) {
        putenv("COLUMNS={$cols}");
        putenv('LINES=40');

        $browser = aligned();

        $method = new ReflectionMethod($browser, 'renderTheme');
        $method->setAccessible(true);
        $method->invoke($browser);
        $browser->emit('key', "\n");

        $browser->status = str_repeat('a status line that goes on and on ', 20);

        foreach (explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser))) as $line) {
            if (mb_strlen($line) > $cols) {
                $over[] = $cols.': '.mb_strlen($line);
            }
        }
    }

    putenv('COLUMNS');
    putenv('LINES');

    expect($over)->toBe([]);
});
