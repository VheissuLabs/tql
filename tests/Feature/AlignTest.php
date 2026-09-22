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
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function aligned(): Browser
{
    $path = sys_get_temp_dir().'/tql-align-'.uniqid().'.sqlite';
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
    config(['tql.ui.sql_always' => true]);

    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', 's');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    $browser->editor->set(str_repeat('a', 400));
    $browser->editor->toEnd();

    expect($method->invoke($browser))->toContain("\e[7m");

    config(['tql.ui.sql_always' => false]);
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
    config(['tql.ui.sql_always' => true]);
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
    config(['tql.ui.sql_always' => false]);

    expect(count($rows))->toBeLessThanOrEqual($lines);
})->with([[24, false], [30, false], [33, false], [40, false], [24, true], [33, true], [60, true]]);

it('never renders wider than the terminal at any width', function () {
    config(['tql.ui.sql_always' => true]);

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
    config(['tql.ui.sql_always' => false]);

    expect($over)->toBe([]);
});

it('does not change height when the sql pane takes focus', function (int $lines) {
    config(['tql.ui.sql_always' => true, 'tql.ui.sql_position' => 'bottom', 'tql.ui.sql_height' => 0]);
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
    config(['tql.ui.sql_always' => false, 'tql.ui.sql_position' => 'top', 'tql.ui.sql_height' => 0]);

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

    // Home then erase down, inside the synchronized block so it is not seen.
    expect($written)->toContain("\e[?2026h\e[H\e[J");
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

    expect($output->fetch())->toContain("\e[H\e[J")
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

it('only offers paging when there is another page', function () {
    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);
    $browser->emit('key', "\n");

    expect($method->invoke($browser))->not->toContain('n/p');

    $browser->hasMore = true;

    expect($method->invoke($browser))->toContain('n/p');
});

it('keeps the hotkey bar to a single line at a usable width', function () {
    putenv('COLUMNS=100');
    putenv('LINES=40');

    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);
    $browser->emit('key', "\n");

    $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser)));

    putenv('COLUMNS');
    putenv('LINES');

    // The hotkey bar is the second to last line; a wrap would push the frame.
    expect(array_filter($lines, fn (string $l) => str_contains($l, 'Pane')))->toHaveCount(1);
});

it('floats the help over the panes instead of interleaving with them', function () {
    config(['tql.ui.sql_always' => true]);
    putenv('COLUMNS=140');
    putenv('LINES=40');

    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', '?');

    $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser)));

    putenv('COLUMNS');
    putenv('LINES');
    config(['tql.ui.sql_always' => false]);

    $plain = implode("\n", $lines);

    // The modal is there, and so is what it is floating over.
    expect($plain)->toContain('HELP')
        ->and($plain)->toContain('TABLES')
        ->and($plain)->toContain('SQL');

    // Every row is still exactly one terminal width, so nothing is doubled up.
    $framed = array_values(array_filter(
        array_map(fn (string $l) => mb_strlen(rtrim($l)), $lines),
        fn (int $w) => $w > 0,
    ));

    expect(max($framed))->toBeLessThanOrEqual(140);
});

it('scrolls the help when it is longer than the modal', function () {
    putenv('COLUMNS=140');
    putenv('LINES=24');

    $browser = aligned();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);
    $browser->emit('key', '?');

    $first = $method->invoke($browser);

    expect($browser->helpIsland->hidden)->toBeGreaterThan(0);

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    $scrolled = $method->invoke($browser);

    putenv('COLUMNS');
    putenv('LINES');

    expect($scrolled)->not->toBe($first)
        ->and($browser->helpOffset)->toBe(2);
});

it('keeps the help modal inside the terminal at any width', function () {
    $over = [];

    foreach ([80, 100, 120, 140, 180, 220] as $cols) {
        foreach ([20, 24, 30, 40] as $rows) {
            putenv("COLUMNS={$cols}");
            putenv("LINES={$rows}");

            $browser = aligned();

            $method = new ReflectionMethod($browser, 'renderTheme');
            $method->setAccessible(true);
            $method->invoke($browser);
            $browser->emit('key', "\n");
            $browser->emit('key', '?');

            foreach (explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser))) as $line) {
                if (mb_strlen($line) > $cols) {
                    $over[] = "{$cols}x{$rows}: ".mb_strlen($line);
                }
            }
        }
    }

    putenv('COLUMNS');
    putenv('LINES');

    expect($over)->toBe([]);
});

it('draws an opaque backdrop behind a modal', function () {
    putenv('COLUMNS=140');
    putenv('LINES=40');

    $browser = aligned();

    widths($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', '?');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser)));

    // Find the modal's own rows, then check the row just above its top border
    // has been blanked where the modal sits rather than showing the panes.
    $top = null;

    foreach ($lines as $index => $line) {
        if (str_contains($line, '┌─ HELP')) {
            $top = $index;
            break;
        }
    }

    expect($top)->not->toBeNull();

    $at = mb_strpos($lines[$top], '┌');
    $above = mb_substr($lines[$top - 1], $at, 10);

    putenv('COLUMNS');
    putenv('LINES');

    expect(trim($above))->toBe('');
});

it('repaints when a modal opens or closes', function () {
    $browser = aligned();

    $render = new ReflectionMethod($browser, 'render');
    $render->setAccessible(true);

    $output = captureAligned();

    $render->invoke($browser);
    $output->fetch();

    $browser->emit('key', '?');
    $render->invoke($browser);

    expect($output->fetch())->toContain("\e[H\e[J");
});

function captureAligned(): BufferedConsoleOutput
{
    $output = new BufferedConsoleOutput;

    Prompt::setOutput($output);

    return $output;
}
