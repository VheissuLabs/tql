<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Support\Paths;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

function sqliteFixture(): string
{
    $path = sys_get_temp_dir().'/dotsql-test-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text, qty integer)');
    $pdo->exec("insert into widgets (name, qty) values ('alpha', 1), ('beta', 2), ('gamma', 3)");
    $pdo->exec('create table no_pk (a text, b text)');
    $pdo->exec("insert into no_pk values ('x', 'y')");

    return $path;
}

function browserFor(string $path, string $name = 'fixture'): Browser
{
    $connection = Connection::create(['name' => $name.uniqid(), 'driver' => 'sqlite', 'database' => $path]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('widgets', $browser->tables, true);

    (new BrowserRenderer($browser))($browser);
    $browser->emit('key', "\n");
    (new BrowserRenderer($browser))($browser);

    return $browser;
}

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

it('quits when q is dispatched through the key event', function () {
    $browser = browserFor(sqliteFixture());

    expect($browser->state)->not->toBe('submit');

    $browser->emit('key', 'q');

    expect($browser->state)->toBe('submit');
});

it('quits on :q typed into the command line', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', ':');
    $browser->emit('key', 'q');

    expect($browser->command)->toBe('q')
        ->and($browser->state)->not->toBe('submit');

    $browser->emit('key', "\n");

    expect($browser->state)->toBe('submit');
});

it('cancels the command line on escape without quitting', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', ':');
    $browser->emit('key', 'x');
    $browser->emit('key', "\e");

    expect($browser->command)->toBeNull();
});

it('moves the row cursor with j and k', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', 'j');
    expect($browser->rowIndex)->toBe(1);

    $browser->emit('key', 'k');
    expect($browser->rowIndex)->toBe(0);
});

it('never renders wider or taller than the terminal', function (int $cols, int $lines) {
    putenv("COLUMNS={$cols}");
    putenv("LINES={$lines}");

    $browser = browserFor(sqliteFixture());
    $frame = (string) (new BrowserRenderer($browser))($browser);

    $stripped = preg_replace('/\e\[[0-9;]*m/', '', $frame);
    $rows = explode("\n", $stripped);

    $widest = max(array_map('mb_strlen', $rows));

    expect($widest)->toBeLessThanOrEqual($cols)
        ->and(count($rows))->toBeLessThanOrEqual($lines);

    putenv('COLUMNS');
    putenv('LINES');
})->with([[80, 24], [110, 30], [160, 40], [240, 60], [320, 80]]);

it('edits a cell and writes it to the database', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->emit('key', 'j');
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    expect($browser->editing)->toBe('beta');

    foreach (str_split('-edited') as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\n");

    $rows = (new PDO('sqlite:'.$path))->query('select id, name from widgets order by id')->fetchAll(PDO::FETCH_ASSOC);

    expect($rows[1]['name'])->toBe('beta-edited')
        ->and($rows[0]['name'])->toBe('alpha')
        ->and($rows[2]['name'])->toBe('gamma');
});

it('leaves the database alone when an edit is cancelled', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->emit('key', 'j');
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', 'Z');
    $browser->emit('key', "\e");

    $name = (new PDO('sqlite:'.$path))->query('select name from widgets where id = 2')->fetchColumn();

    expect($browser->editing)->toBeNull()
        ->and($name)->toBe('beta');
});

it('refuses to edit a table with no single-column primary key', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->tableIndex = array_search('no_pk', $browser->tables, true);
    $browser->emit('key', "\n");
    $browser->focus = 'grid';

    $browser->emit('key', 'e');

    expect($browser->editing)->toBeNull()
        ->and($browser->status)->toContain('no single-column primary key');
});

it('refuses to edit a read-only connection', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->connection->forceFill(['read_only' => true])->save();
    $browser->focus = 'grid';

    $browser->emit('key', 'e');

    expect($browser->editing)->toBeNull()
        ->and($browser->status)->toContain('read-only');
});

function frameOf(Browser $browser): string
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser));
}

function headerRowOf(Browser $browser): string
{
    foreach (explode("\n", frameOf($browser)) as $line) {
        if (str_contains($line, 'name') && str_contains($line, 'qty')) {
            return $line;
        }
    }

    return '';
}

it('widens the rendered column when resized', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', 'l');
    $before = headerRowOf($browser);

    $browser->emit('key', '>');
    $after = headerRowOf($browser);

    expect($browser->widthOverrides)->toHaveKey('name')
        ->and($after)->not->toBe($before)
        ->and($before)->not->toBe('');

    $browser->emit('key', '=');

    expect($browser->widthOverrides)->not->toHaveKey('name')
        ->and(headerRowOf($browser))->toBe($before);
});

it('resizes columns even while the sidebar has focus', function () {
    $browser = browserFor(sqliteFixture());

    $browser->focus = 'sidebar';
    $browser->emit('key', '>');

    expect($browser->widthOverrides)->not->toBeEmpty();
});

it('resizes a column by dragging its border', function () {
    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $row = $browser->table->y + 1;
    $handle = $browser->columnHandles[1];

    $browser->emit('key', "\e[<0;{$handle};{$row}M");

    expect($browser->columnIndex)->toBe(1);

    $browser->emit('key', "\e[<32;".($handle + 6).";{$row}M");

    expect($browser->widthOverrides)->toHaveKey('name');

    $widened = $browser->widthOverrides['name'];

    $browser->emit('key', "\e[<0;".($handle + 6).";{$row}m");
    $browser->emit('key', "\e[<32;2;{$row}M");

    expect($browser->widthOverrides['name'])->toBe($widened);
});

it('keeps several column widths at once', function () {
    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $browser->emit('key', 'l');
    $browser->emit('key', '>');
    $browser->emit('key', 'l');
    $browser->emit('key', '>');

    expect($browser->widthOverrides)->toHaveKeys(['name', 'qty']);

    frameOf($browser);

    expect($browser->widthOverrides)->toHaveKeys(['name', 'qty']);
});

it('does not scroll earlier columns away when moving right', function () {
    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $browser->emit('key', 'l');
    $browser->emit('key', 'l');

    expect(headerRowOf($browser))->toContain('id');
});

it('clicks the table the user actually sees in the sidebar', function () {
    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $sidebar = $browser->sidebar;

    expect($sidebar)->not->toBeNull();

    foreach ($browser->tables as $index => $table) {
        $terminalRow = $sidebar->y + 1 + ($index - $sidebar->start);

        $fresh = browserFor(sqliteFixture());
        frameOf($fresh);

        $fresh->emit('key', "\e[<0;".($sidebar->x + 2).";{$terminalRow}M");

        expect($fresh->tables[$fresh->tableIndex])->toBe($table);
    }
});

it('ignores sidebar clicks on the chrome above the first row', function () {
    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $before = $browser->tableIndex;

    $browser->emit('key', "\e[<0;10;".($browser->sidebar->y).'M');

    expect($browser->tableIndex)->toBe($before);
});

it('draws each island exactly where it claims to be', function () {
    $browser = browserFor(sqliteFixture());
    $frame = frameOf($browser);
    $lines = explode("\n", $frame);

    $sidebarRow = null;

    foreach ($lines as $index => $line) {
        if (str_contains($line, 'TABLES')) {
            $sidebarRow = $index + 1;
            break;
        }
    }

    expect($sidebarRow)->not->toBeNull()
        ->and($browser->sidebar->y)->toBe($sidebarRow);
});

it('highlights the selected table in the sidebar', function () {
    $browser = browserFor(sqliteFixture());

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);

    $selected = $browser->tables[$browser->tableIndex];

    expect($raw)->toContain("\e[7m");

    foreach (explode("\n", $raw) as $line) {
        if (str_contains($line, $selected) && str_contains($line, "\e[7m")) {
            expect(true)->toBeTrue();

            return;
        }
    }

    throw new Exception("the selected table [{$selected}] is not highlighted");
});

it('honours the configured top margin and keeps coordinates honest', function (int $margin) {
    config(['dotsql.ui.top_margin' => $margin]);

    $browser = browserFor(sqliteFixture());
    $lines = explode("\n", frameOf($browser));

    $blank = 0;

    foreach ($lines as $line) {
        if (trim($line) !== '') {
            break;
        }

        $blank++;
    }

    $drawn = null;

    foreach ($lines as $index => $line) {
        if (str_contains($line, 'TABLES')) {
            $drawn = $index + 1;
            break;
        }
    }

    expect($blank)->toBe($margin)
        ->and($browser->sidebar->y)->toBe($drawn);

    config(['dotsql.ui.top_margin' => 1]);
})->with([0, 1, 3]);

it('lets a column use the space when nothing competes for it', function () {
    putenv('COLUMNS=160');
    putenv('LINES=24');

    $path = sys_get_temp_dir().'/dotsql-wide-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table notes (id integer primary key, body text)');
    $pdo->exec("insert into notes (body) values ('".str_repeat('x', 60)."')");

    $connection = Connection::create([
        'name' => 'wide'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('notes', $browser->tables, true);
    frameOf($browser);
    $browser->emit('key', "\n");

    $frame = frameOf($browser);

    putenv('COLUMNS');
    putenv('LINES');

    expect($frame)->toContain(str_repeat('x', 60));
});

it('still truncates when columns compete for width', function () {
    $path = sys_get_temp_dir().'/dotsql-narrow-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table wide (id integer primary key, a text, b text, c text, d text, e text)');
    $long = str_repeat('y', 60);
    $pdo->exec("insert into wide (a,b,c,d,e) values ('{$long}','{$long}','{$long}','{$long}','{$long}')");

    $connection = Connection::create([
        'name' => 'narrow'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('wide', $browser->tables, true);
    frameOf($browser);
    $browser->emit('key', "\n");

    expect(frameOf($browser))->toContain('…');
});

it('inspects the selected cell in full', function () {
    $path = sys_get_temp_dir().'/dotsql-inspect-'.uniqid().'.sqlite';
    touch($path);

    $long = 'a very long value that will certainly be truncated in the grid because it keeps going';

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table notes (id integer primary key, body text)');
    $pdo->exec("insert into notes (body) values ('{$long}')");

    $connection = Connection::create([
        'name' => 'inspect'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('notes', $browser->tables, true);
    frameOf($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', 'l');

    expect($browser->cellColumn())->toBe('body')
        ->and($browser->cellText())->toBe($long);

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('inspect');

    $frame = frameOf($browser);

    expect($frame)->toContain('keeps going');

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse');
});

it('does not open the inspector from the sidebar', function () {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('browse');
});

it('merges a user config file over the shipped defaults', function () {
    $directory = Paths::ensureDirectory();
    $file = Paths::configFile();

    file_put_contents($file, "<?php return ['ui' => ['mouse_row_offset' => 4]];");

    $shipped = require base_path('config/dotsql.php');

    config(['dotsql' => $shipped]);

    $user = require $file;
    config(['dotsql' => array_replace_recursive(config('dotsql'), $user)]);

    expect(config('dotsql.ui.mouse_row_offset'))->toBe(4)
        ->and(config('dotsql.ui.top_margin'))->toBe($shipped['ui']['top_margin']);

    unlink($file);
});

it('aligns the status and hotkey lines with the island content', function () {
    $browser = browserFor(sqliteFixture());
    $lines = array_values(array_filter(explode("\n", frameOf($browser)), fn ($l) => trim($l) !== ''));

    $contentColumn = $browser->sidebar->x + 1;

    $status = null;
    $hotkeys = null;

    foreach ($lines as $line) {
        if (str_contains($line, '·') && ! str_contains($line, '┌')) {
            $status ??= $line;
        }

        if (str_contains($line, 'Quit')) {
            $hotkeys ??= $line;
        }
    }

    expect($status)->not->toBeNull()
        ->and($hotkeys)->not->toBeNull();

    $indent = fn (string $l) => strlen($l) - strlen(ltrim($l)) + 1;

    expect($indent($status))->toBe($contentColumn)
        ->and($indent($hotkeys))->toBe($contentColumn);
});

it('marks the selected row without underlining it', function () {
    config(['dotsql.ui.row_style' => 'marker']);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 'j');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);

    expect($raw)->toContain('▸')
        ->and($raw)->not->toContain("\e[4m");

    config(['dotsql.ui.row_style' => 'marker']);
});

it('supports the other selected row styles', function (string $style, string $expected) {
    config(['dotsql.ui.row_style' => $style]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 'j');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    expect($method->invoke($browser))->toContain($expected);

    config(['dotsql.ui.row_style' => 'marker']);
})->with([
    ['underline', "\e[4m"],
    ['bold', "\e[1m"],
    ['inverse', "\e[7m"],
]);

it('does not jump when a column border is first grabbed', function () {
    config(['dotsql.ui.mouse_row_offset' => 0]);

    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $row = $browser->table->y + 1;
    $handle = $browser->columnHandles[1];

    $browser->emit('key', "\e[<0;{$handle};{$row}M");
    frameOf($browser);

    expect($browser->columnHandles[1])->toBe($handle)
        ->and($browser->widthOverrides)->toBeEmpty();
});

it('keeps the column border under the pointer while dragging', function () {
    config(['dotsql.ui.mouse_row_offset' => 0]);
    putenv('COLUMNS=140');
    putenv('LINES=24');

    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $row = $browser->table->y + 1;
    $handle = $browser->columnHandles[1];

    $browser->emit('key', "\e[<0;{$handle};{$row}M");
    frameOf($browser);

    $misses = 0;

    foreach ([$handle + 8, $handle + 16, $handle + 3, $handle + 30] as $target) {
        $browser->emit('key', "\e[<32;{$target};{$row}M");
        frameOf($browser);

        if (abs(($browser->columnHandles[1] ?? -99) - $target) > 1) {
            $misses++;
        }
    }

    putenv('COLUMNS');
    putenv('LINES');

    expect($misses)->toBe(0);
});

it('clamps a column to a minimum width instead of inverting it', function () {
    config(['dotsql.ui.mouse_row_offset' => 0]);

    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $row = $browser->table->y + 1;
    $handle = $browser->columnHandles[1];

    $browser->emit('key', "\e[<0;{$handle};{$row}M");
    frameOf($browser);

    $browser->emit('key', "\e[<32;1;{$row}M");
    frameOf($browser);

    expect($browser->widthOverrides['name'])->toBe(3);
});

it('shows help and lists the commands', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', '?');

    expect($browser->mode)->toBe('help');

    $frame = frameOf($browser);

    expect($frame)->toContain('HELP')
        ->and($frame)->toContain(':export')
        ->and($frame)->toContain('drag')
        ->and(substr_count($frame, 'HELP'))->toBe(1);

    $browser->emit('key', '?');

    expect($browser->mode)->toBe('browse');
});

function jsonBrowser(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-json-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, payload text, plain text)');
    $pdo->prepare('insert into events (payload, plain) values (?, ?)')->execute([
        json_encode(['user' => ['id' => 42, 'roles' => ['admin', 'owner']], 'ok' => true, 'score' => 9.75, 'nil' => null]),
        'just a string',
    ]);

    $connection = Connection::create([
        'name' => 'json'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('events', $browser->tables, true);
    frameOf($browser);
    $browser->emit('key', "\n");

    return $browser;
}

it('detects a json column', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');

    expect($browser->cellColumn())->toBe('payload')
        ->and($browser->cellIsJson())->toBeTrue();

    $browser->emit('key', 'l');

    expect($browser->cellColumn())->toBe('plain')
        ->and($browser->cellIsJson())->toBeFalse();
});

it('inspects json with line numbers and highlighting', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);
    $plain = preg_replace('/\e\[[0-9;]*m/', '', $raw);

    expect($plain)->toContain('json')
        ->and($plain)->toContain('  1 ')
        ->and($plain)->toContain('  2 ')
        ->and($raw)->toContain("\e[36m")
        ->and($raw)->toContain("\e[32m")
        ->and($raw)->toContain("\e[33m")
        ->and($raw)->toContain("\e[35m");
});

it('hides the grid while inspecting json', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    expect(frameOf($browser))->not->toContain('TABLES');
});

it('scrolls through a long json document', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    expect($browser->inspectOffset)->toBe(0);

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($browser->inspectOffset)->toBe(2);

    $browser->emit('key', 'k');

    expect($browser->inspectOffset)->toBe(1);

    $browser->emit('key', 'p');

    expect($browser->inspectOffset)->toBe(0);
});

it('uses the plain inspector for non-json values', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    $frame = frameOf($browser);

    expect($frame)->toContain('just a string')
        ->and($frame)->toContain('TABLES');
});

it('loads the table as the sidebar cursor moves', function () {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';
    $browser->emit('key', 'k');
    $browser->emit('key', 'k');

    $first = $browser->currentTable();
    $firstHeaders = $browser->headers;

    $browser->emit('key', 'j');

    expect($browser->currentTable())->not->toBe($first)
        ->and($browser->headers)->not->toBe($firstHeaders);
});

it('never shows one table under another table name', function () {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';

    foreach (range(1, count($browser->tables)) as $ignored) {
        $browser->emit('key', 'j');

        $frame = frameOf($browser);
        $expected = $browser->currentTable();

        expect($frame)->toContain($expected);

        foreach ($browser->headers as $header) {
            expect($frame)->toContain($header);
        }
    }
});

it('joins the column separators to the island borders', function () {
    $browser = browserFor(sqliteFixture());
    $lines = explode("\n", frameOf($browser));

    $top = null;
    $rule = null;
    $bottom = null;

    foreach ($lines as $line) {
        if ($top === null && str_contains($line, '┬')) {
            $top = $line;
        }

        if ($rule === null && str_contains($line, '┼')) {
            $rule = $line;
        }

        if (str_contains($line, '┴')) {
            $bottom = $line;
        }
    }

    expect($top)->not->toBeNull('top border has no ┬ join')
        ->and($rule)->not->toBeNull('header rule has no ┼ join')
        ->and($bottom)->not->toBeNull('bottom border has no ┴ join')
        ->and($rule)->toContain('├')
        ->and($rule)->toContain('┤');
});

it('lines up the joins in the borders with the separators in the rows', function () {
    $browser = browserFor(sqliteFixture());
    $lines = array_values(array_filter(
        explode("\n", frameOf($browser)),
        fn ($l) => str_contains($l, '┬') || str_contains($l, '┼') || str_contains($l, '┴')
    ));

    $positions = function (string $line, array $needles) {
        $found = [];

        foreach (preg_split('//u', $line, -1, PREG_SPLIT_NO_EMPTY) as $index => $char) {
            if (in_array($char, $needles, true)) {
                $found[] = $index;
            }
        }

        return $found;
    };

    $top = $positions($lines[0], ['┬']);
    $rule = $positions($lines[1], ['┼']);
    $bottom = $positions($lines[count($lines) - 1], ['┴']);

    expect($rule)->not->toBeEmpty()
        ->and($bottom)->toBe($rule, 'the bottom border should join every separator')
        ->and(array_diff($top, $rule))->toBe([], 'the top border should only join where separators are');

    $hidden = array_diff($rule, $top);

    foreach ($hidden as $position) {
        expect($position)->toBeLessThan(max($top) ?: PHP_INT_MAX, 'only joins under the title may be missing');
    }
});

it('keeps the row marker out of the cell padding', function () {
    config(['dotsql.ui.row_style' => 'marker']);

    $browser = browserFor(sqliteFixture());
    $lines = explode("\n", frameOf($browser));

    foreach ($lines as $line) {
        if (! str_contains($line, '▸')) {
            continue;
        }

        $at = mb_strpos($line, '▸');
        $before = mb_substr($line, $at - 1, 1);
        $after = mb_substr($line, $at + 1, 1);

        expect($before)->toBe(' ', 'the marker should not touch the border')
            ->and($after)->toBe(' ', 'the marker should not touch the value');

        return;
    }

    throw new Exception('no marker found in the frame');
});

it('inspects from the sidebar without needing to enter the grid first', function () {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('inspect')
        ->and($browser->focus)->toBe('grid');
});

it('says so when there is nothing to inspect', function () {
    $path = sys_get_temp_dir().'/dotsql-empty-'.uniqid().'.sqlite';
    touch($path);

    (new PDO('sqlite:'.$path))->exec('create table blanks (id integer primary key)');

    $connection = Connection::create([
        'name' => 'empty'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    frameOf($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('browse')
        ->and($browser->status)->toContain('nothing to inspect');

    unlink($path);
});
