<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Support\Paths;
use App\Tui\Browser;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\Styler;
use App\Tui\Layout;
use App\Tui\QueryEditor;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\Config;
use Laravel\Prompts\Key;

function sqliteFixture(): string
{
    $path = sys_get_temp_dir().'/tql-test-'.uniqid().'.sqlite';
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

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellEditor->buffer())->toBe('beta');

    foreach (str_split('-edited') as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\x04");

    // An edit is pending until :w, the same as a deletion.
    $browser->emit('key', ':');
    $browser->emit('key', 'w');
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

    expect($browser->cellEditor)->toBeNull()
        ->and($browser->mode)->toBe('browse')
        ->and($name)->toBe('beta');
});

it('refuses to edit a table with no single-column primary key', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->tableIndex = array_search('no_pk', $browser->tables, true);
    $browser->emit('key', "\n");
    $browser->focus = 'grid';

    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->editable)->toBeFalse()
        ->and($browser->readOnlyReason)->toContain('no single-column primary key');
});

it('refuses to edit a read-only connection', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $browser->connection->forceFill(['read_only' => true])->save();
    $browser->focus = 'grid';

    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->editable)->toBeFalse()
        ->and($browser->readOnlyReason)->toContain('read-only');
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

it('resizes the table list, not a column, while the table list has focus', function () {
    $browser = browserFor(sqliteFixture());
    $start = $browser->tablesWidth();

    $browser->focus = 'sidebar';
    $browser->emit('key', '>');

    expect($browser->widthOverrides)->toBeEmpty()
        ->and($browser->tablesWidth())->toBe($start + 4);
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
        if (str_starts_with($line, '┌─ ')) {
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
    config(['tql.ui.top_margin' => $margin]);

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
        if (str_starts_with($line, '┌─ ')) {
            $drawn = $index + 1;
            break;
        }
    }

    expect($blank)->toBe($margin)
        ->and($browser->sidebar->y)->toBe($drawn);

    config(['tql.ui.top_margin' => 1]);
})->with([0, 1, 3]);

it('lets a column use the space when nothing competes for it', function () {
    putenv('COLUMNS=160');
    putenv('LINES=24');

    $path = sys_get_temp_dir().'/tql-wide-'.uniqid().'.sqlite';
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
    $path = sys_get_temp_dir().'/tql-narrow-'.uniqid().'.sqlite';
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

it('merges a user config file over the shipped defaults', function () {
    $directory = Paths::ensureDirectory();
    $file = Paths::configFile();

    file_put_contents($file, "<?php return ['ui' => ['mouse_row_offset' => 4]];");

    $shipped = require base_path('config/tql.php');

    config(['tql' => $shipped]);

    $user = require $file;
    config(['tql' => array_replace_recursive(config('tql'), $user)]);

    expect(config('tql.ui.mouse_row_offset'))->toBe(4)
        ->and(config('tql.ui.top_margin'))->toBe($shipped['ui']['top_margin']);

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

        if (str_contains($line, 'Help')) {
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
    config(['tql.ui.row_style' => 'marker']);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 'j');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);

    expect($raw)->toContain('▸')
        ->and($raw)->not->toContain("\e[4m");

    config(['tql.ui.row_style' => 'marker']);
});

it('supports the other selected row styles', function (string $style, string $expected) {
    config(['tql.ui.row_style' => $style]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 'j');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    expect($method->invoke($browser))->toContain($expected);

    config(['tql.ui.row_style' => 'marker']);
})->with([
    ['underline', "\e[4m"],
    ['bold', "\e[1m"],
    ['inverse', "\e[7m"],
]);

it('does not jump when a column border is first grabbed', function () {
    config(['tql.ui.mouse_row_offset' => 0]);

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
    config(['tql.ui.mouse_row_offset' => 0]);
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
    config(['tql.ui.mouse_row_offset' => 0]);

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
        ->and($frame)->toContain('tab')
        ->and(substr_count($frame, 'HELP'))->toBe(1)
        ->and($frame)->toContain(':export')
        ->and($browser->helpIsland->hidden)->toBe(0);

    $browser->emit('key', '?');

    expect($browser->mode)->toBe('browse');
});

function jsonBrowser(): Browser
{
    $path = sys_get_temp_dir().'/tql-json-'.uniqid().'.sqlite';
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
    config(['tql.ui.row_style' => 'marker']);

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

it('quits the application on :q', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', ':');
    $browser->emit('key', 'q');
    $browser->emit('key', "\n");

    expect($browser->state)->toBe('submit')
        ->and($browser->value())->toBe('quit');
});

it('returns to the connection list on :c', function () {
    $browser = browserFor(sqliteFixture());

    foreach (str_split(':c') as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\n");

    expect($browser->state)->toBe('submit')
        ->and($browser->value())->toBe('connections');
});

it('opens a plain value with the cursor at the end', function () {
    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    expect($browser->cellEditor->cursor())->toBe(mb_strlen($browser->cellEditor->buffer()));
});

it('opens json with the cursor at the top', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    expect($browser->editingJson)->toBeTrue()
        ->and($browser->cellEditor->cursor())->toBe(0);
});

it('refuses to save invalid json', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');
    $browser->emit('key', 'x');
    $browser->emit('key', "\x04");

    expect($browser->mode)->toBe('edit')
        ->and($browser->status)->toContain('not valid json');
});

it('shows a cursor in the value editor without shifting the text', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    expect($browser->editable)->toBeTrue();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    expect($method->invoke($browser))->toContain("\e[7m");

    $indent = function (string $frame, int $line) {
        $rows = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $frame));
        $row = $rows[$line] ?? '';

        return mb_strpos($row, '"');
    };

    $before = frameOf($browser);

    $browser->emit('key', "\x1b[B");
    $browser->emit('key', "\x1b[B");

    $after = frameOf($browser);

    foreach ([3, 4, 5] as $line) {
        expect($indent($after, $line))->toBe($indent($before, $line));
    }
});

it('opens a value read-only when it cannot be written back', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);
    $browser->emit('key', "\e");
    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->editable)->toBeFalse()
        ->and($browser->readOnlyReason)->toContain('query results');

    expect(frameOf($browser))->toContain('read-only');
});

it('ignores typing in a read-only value but still scrolls', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);
    $browser->emit('key', "\e");
    $browser->emit('key', 'e');

    $before = $browser->cellEditor->buffer();

    $browser->emit('key', 'z');
    $browser->emit('key', 'z');

    expect($browser->cellEditor->buffer())->toBe($before);

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse');
});

it('opens read-only with I even where editing is possible', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'I');

    expect($browser->mode)->toBe('edit')
        ->and($browser->editable)->toBeFalse();

    $before = $browser->cellEditor->buffer();
    $browser->emit('key', 'z');

    expect($browser->cellEditor->buffer())->toBe($before);
});

it('opens editable with e where editing is possible', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->editable)->toBeTrue()
        ->and($browser->readOnlyReason)->toBeNull();
});

it('edits a value in a modal over the grid, sized to the value', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    $frame = frameOf($browser);
    $island = $browser->valueIsland;

    expect($browser->mode)->toBe('edit')
        ->and($island->modal)->toBeTrue()
        ->and($island->width)->toBeLessThan($browser->terminal()->cols())
        ->and($island->innerHeight())->toBe(1)
        ->and($frame)->toMatch('/^┌─ /m')
        ->and($frame)->toContain('events');
});

it('widens the value editor for a long line, up to the frame', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'l');
    $browser->emit('key', 'e');

    foreach (mb_str_split(str_repeat('x', 400)) as $char) {
        $browser->emit('key', $char);
    }

    frameOf($browser);

    expect($browser->valueIsland->width)->toBe($browser->terminal()->cols() - 4);
});

it('hides the cursor when the value is read-only', function () {
    $browser = jsonBrowser();
    $browser->emit('key', 'l');
    $browser->emit('key', 'I');

    expect($browser->editable)->toBeFalse()
        ->and(editorShown($browser))->not->toContain("\e[7m");
});

it('puts the sql pane where the config says', function (string $position, bool $sqlFirst) {
    config(['tql.ui.sql_position' => $position]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 's');

    $lines = explode("\n", frameOf($browser));

    $sqlAt = null;
    $tableAt = null;

    foreach ($lines as $index => $line) {
        if ($sqlAt === null && str_contains($line, 'SQL')) {
            $sqlAt = $index;
        }

        if ($tableAt === null && str_contains($line, 'widgets')) {
            $tableAt = $index;
        }
    }

    expect($sqlAt)->not->toBeNull()
        ->and($tableAt)->not->toBeNull()
        ->and($sqlAt < $tableAt)->toBe($sqlFirst);

    config(['tql.ui.sql_position' => 'top']);
})->with([
    ['top', true],
    ['bottom', false],
]);

it('ignores a nonsense sql position', function () {
    config(['tql.ui.sql_position' => 'sideways']);

    expect(Layout::sqlPosition())->toBe('top');

    config(['tql.ui.sql_position' => 'top']);
});

it('keeps the sql pane visible when configured to', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());

    expect($browser->mode)->toBe('browse')
        ->and(frameOf($browser))->toContain('─ [3] SQL ');

    $browser->emit('key', 's');

    expect($browser->mode)->toBe('query');

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse')
        ->and(frameOf($browser))->toContain('─ [3] SQL ');

    config(['tql.ui.sql_always' => false]);
});

it('hides the sql pane by default until s is pressed', function () {
    $browser = browserFor(sqliteFixture());

    expect(frameOf($browser))->not->toContain('─ [3] SQL ');

    $browser->emit('key', 's');

    expect(frameOf($browser))->toContain('─ [3] SQL ');
});

it('only shows the editor cursor when the editor has focus', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());

    $inverses = function () use ($browser) {
        $method = new ReflectionMethod($browser, 'renderTheme');
        $method->setAccessible(true);

        return substr_count($method->invoke($browser), "\e[7m");
    };

    $unfocused = $inverses();

    $browser->emit('key', 's');

    // The editor gains a cursor and the grid loses one, so the count holds.
    expect($inverses())->toBe($unfocused);

    config(['tql.ui.sql_always' => false]);
});
it('honours a configured sql height', function () {
    config(['tql.ui.sql_always' => true, 'tql.ui.sql_height' => 6]);

    $browser = browserFor(sqliteFixture());
    $lines = explode("\n", frameOf($browser));

    $start = null;
    $end = null;

    foreach ($lines as $index => $line) {
        if (str_contains($line, 'SQL')) {
            $start = $index;
        }

        if ($start !== null && $index > $start && str_contains($line, '└')) {
            $end = $index;
            break;
        }
    }

    expect($end - $start + 1)->toBe(6);

    config(['tql.ui.sql_always' => false, 'tql.ui.sql_height' => 0]);
});

it('shows the query behind the current view', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());
    $frame = frameOf($browser);

    expect($frame)->toContain('select * from')
        ->and($frame)->toContain('widgets');

    config(['tql.ui.sql_always' => false]);
});

it('does not show the internal extra row in the query', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());

    expect($browser->lastStatement)->toContain('limit '.Browser::PAGE)
        ->and($browser->lastStatement)->not->toContain('limit '.(Browser::PAGE + 1));

    config(['tql.ui.sql_always' => false]);
});

it('updates the shown query when you page', function () {
    config(['tql.ui.sql_always' => true]);

    $path = sys_get_temp_dir().'/tql-page-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table many (id integer primary key, v text)');
    $statement = $pdo->prepare('insert into many (v) values (?)');

    foreach (range(1, Browser::PAGE + 20) as $i) {
        $statement->execute(['row '.$i]);
    }

    $connection = Connection::create([
        'name' => 'page'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    frameOf($browser);
    $browser->emit('key', "\n");

    expect($browser->lastStatement)->toContain('offset 0');

    $browser->emit('key', 'n');

    expect($browser->lastStatement)->toContain('offset '.Browser::PAGE);

    unlink($path);
    config(['tql.ui.sql_always' => false]);
});

it('gives the pane back to your own query when you start typing', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 's');

    foreach (str_split('select 1') as $char) {
        $browser->emit('key', $char);
    }

    $frame = frameOf($browser);

    expect($frame)->toContain('select 1');

    config(['tql.ui.sql_always' => false]);
});

it('hands you the current query to edit when you press s', function () {
    $browser = browserFor(sqliteFixture());

    expect($browser->lastStatement)->toContain('select * from');

    $browser->emit('key', 's');

    expect($browser->editor->buffer())->toBe($browser->lastStatement);
});

it('does not clobber a query you were already writing', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', 's');

    foreach (str_split(' extra') as $char) {
        $browser->emit('key', $char);
    }

    $typed = $browser->editor->buffer();

    $browser->emit('key', "\e");
    $browser->emit('key', 's');

    expect($browser->editor->buffer())->toBe($typed);
});

it('runs an edited query against the connection', function () {
    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 's');

    $browser->editor->set("select name from widgets where name = 'beta'");
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->resultsFromQuery)->toBeTrue()
        ->and($browser->headers)->toBe(['name'])
        ->and($browser->rows)->toHaveCount(1)
        ->and($browser->rows[0]['name'])->toBe('beta');
});

function twoTableBrowser(): Browser
{
    $path = sys_get_temp_dir().'/tql-two-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, name text)');
    $pdo->exec('create table settings (id integer primary key, team text)');
    $pdo->exec("insert into events (name) values ('one')");
    $pdo->exec("insert into settings (team) values ('notarydash')");

    $connection = Connection::create([
        'name' => 'two'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    $browser->tableIndex = array_search('events', $browser->tables, true);
    frameOf($browser);
    $browser->emit('key', "\n");

    return $browser;
}

it('moves the sidebar to the table a query selects from', function () {
    $browser = twoTableBrowser();

    expect($browser->currentTable())->toBe('events');

    $browser->emit('key', 's');
    $browser->editor->set('select * from settings');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->currentTable())->toBe('settings');
});

it('handles quoted table names', function () {
    $browser = twoTableBrowser();

    $browser->emit('key', 's');
    $browser->editor->set('select * from "settings" limit 10');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->currentTable())->toBe('settings');
});

it('leaves the sidebar alone for a query with no known table', function () {
    $browser = twoTableBrowser();
    $before = $browser->currentTable();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->currentTable())->toBe($before);
});

it('titles the pane with the table a query selects from', function () {
    $browser = twoTableBrowser();

    $browser->emit('key', 's');
    $browser->editor->set('select * from "settings"');
    $browser->emit('key', QueryEditor::RUN);

    $frame = frameOf($browser);

    expect($frame)->toContain('─ [2] settings ')
        ->and($frame)->not->toContain('RESULTS');
});

it('falls back to RESULTS when the query has no known table', function () {
    $browser = twoTableBrowser();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);

    expect(frameOf($browser))->toContain('RESULTS');
});

it('goes back to the table name when you leave the results', function () {
    $browser = twoTableBrowser();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);
    $browser->emit('key', "\e");
    $browser->emit('key', 'r');

    expect($browser->queryTable)->toBeNull()
        ->and(frameOf($browser))->toContain('─ [2] events ');
});

it('tabs through the sql pane when it is on screen', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';

    $browser->emit('key', "\t");

    expect($browser->focus)->toBe('grid')
        ->and($browser->mode)->toBe('browse');

    $browser->emit('key', "\t");

    expect($browser->mode)->toBe('query');

    $browser->emit('key', "\t");

    expect($browser->mode)->toBe('browse')
        ->and($browser->focus)->toBe('sidebar');

    config(['tql.ui.sql_always' => false]);
});

it('moves from the table list to the grid with the right arrow', function (string $key) {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';
    $browser->columnIndex = 0;

    $browser->emit('key', $key);

    expect($browser->focus)->toBe('grid')
        ->and($browser->columnIndex)->toBe(0);

    $browser->emit('key', $key);

    expect($browser->focus)->toBe('grid')
        ->and($browser->columnIndex)->toBe(1);
})->with([Key::RIGHT_ARROW, 'l']);

it('tabs between two panes when the sql pane is hidden', function () {
    $browser = browserFor(sqliteFixture());
    $browser->focus = 'sidebar';

    $browser->emit('key', "\t");

    expect($browser->focus)->toBe('grid');

    $browser->emit('key', "\t");

    expect($browser->focus)->toBe('sidebar')
        ->and($browser->mode)->toBe('browse');
});

it('leaves the sql pane with tab rather than indenting', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', 's');

    $before = $browser->editor->buffer();

    $browser->emit('key', "\t");

    expect($browser->editor->buffer())->toBe($before)
        ->and($browser->mode)->toBe('browse');

    config(['tql.ui.sql_always' => false]);
});

it('highlights the query it is showing', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);

    $sqlLine = '';

    foreach (explode("\n", $raw) as $line) {
        if (str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 'select * from')) {
            $sqlLine = $line;
            break;
        }
    }

    expect($sqlLine)->toContain("\e[35m")
        ->and($sqlLine)->toContain("\e[36m");

    config(['tql.ui.sql_always' => false]);
});

it('gives spare width to the columns that are truncated', function () {
    putenv('COLUMNS=140');
    putenv('LINES=20');

    $path = sys_get_temp_dir().'/tql-width-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table t (id integer primary key, long text, stamp text)');
    $pdo->prepare('insert into t (long, stamp) values (?, ?)')->execute([
        str_repeat('x', 120),
        '2026-09-22T15:11:32+00:00',
    ]);

    $connection = Connection::create([
        'name' => 'width'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    frameOf($browser);
    $browser->emit('key', "\n");
    frameOf($browser);

    $stamp = array_search('stamp', $browser->headers, true);
    $long = array_search('long', $browser->headers, true);

    $stampWidth = $browser->table->widthOf($stamp);
    $longWidth = $browser->table->widthOf($long);

    expect($stampWidth)->toBe(25)
        ->and($longWidth)->toBeGreaterThan(28);

    putenv('COLUMNS');
    putenv('LINES');
    unlink($path);
});

it('does not redistribute width while a column is being dragged', function () {
    config(['tql.ui.mouse_row_offset' => 0]);
    putenv('COLUMNS=140');
    putenv('LINES=24');

    $browser = browserFor(sqliteFixture());
    frameOf($browser);

    $row = $browser->table->y + 1;
    $handle = $browser->columnHandles[1];

    $browser->emit('key', "\e[<0;{$handle};{$row}M");
    frameOf($browser);

    $firstColumn = $browser->table->widthOf(0);

    $browser->emit('key', "\e[<32;".($handle - 6).";{$row}M");
    frameOf($browser);

    expect($browser->table->widthOf(0))->toBe($firstColumn);

    putenv('COLUMNS');
    putenv('LINES');
});

it('keeps an unedited query in step with the table', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = twoTableBrowser();
    $browser->emit('key', 's');

    expect($browser->editor->buffer())->toContain('events');

    $browser->emit('key', "\e");
    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->currentTable())->toBe('settings')
        ->and($browser->editor->buffer())->toContain('settings')
        ->and($browser->editor->buffer())->toBe($browser->lastStatement);

    config(['tql.ui.sql_always' => false]);
});

it('resyncs even a query you edited once you browse away', function () {
    $browser = twoTableBrowser();
    $browser->emit('key', 's');
    $browser->editor->set('select * from "events" where id > 2');
    $browser->emit('key', "\e");

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    expect($browser->currentTable())->toBe('settings')
        ->and($browser->editor->buffer())->toBe($browser->lastStatement)
        ->and($browser->editor->buffer())->toContain('settings');
});

it('does not overwrite what you are typing', function () {
    $browser = twoTableBrowser();
    $browser->emit('key', 's');
    $browser->editor->set('select * from "events" where id > 2');

    expect($browser->mode)->toBe('query');

    $browser->emit('key', 'r');

    expect($browser->editor->buffer())->toContain('where id > 2');
});

it('keeps the query in step when paging', function () {
    config(['tql.ui.sql_always' => true]);

    $path = sys_get_temp_dir().'/tql-sync-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table many (id integer primary key)');

    foreach (range(1, Browser::PAGE + 10) as $ignored) {
        $pdo->exec('insert into many default values');
    }

    $connection = Connection::create([
        'name' => 'sync'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    frameOf($browser);
    $browser->emit('key', "\n");
    $browser->emit('key', 's');
    $browser->emit('key', "\e");

    $browser->emit('key', 'n');

    expect($browser->editor->buffer())->toContain('offset '.Browser::PAGE);

    unlink($path);
    config(['tql.ui.sql_always' => false]);
});

it('goes back to the table from query results with escape', function () {
    $browser = jsonBrowser();

    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);

    expect($browser->resultsFromQuery)->toBeTrue()
        ->and($browser->headers)->toBe(['one']);

    $browser->emit('key', "\e");   // out of the editor
    $browser->emit('key', "\e");   // and back to the table

    expect($browser->resultsFromQuery)->toBeFalse()
        ->and($browser->status)->toContain('back to');

    // And the status line says so while the results are up.
    $browser->emit('key', 's');
    $browser->editor->set('select 1 as one');
    $browser->emit('key', QueryEditor::RUN);
    $browser->emit('key', "\e");

    expect(frameOf($browser))->toContain('esc goes back to');
});

it('lights the status up when it changes, and settles after', function () {
    $browser = jsonBrowser();

    $browser->emit('key', 'd');   // mark a row

    expect($browser->statusFresh)->toBeTrue();

    $painted = function () use ($browser) {
        $method = new ReflectionMethod($browser, 'renderTheme');
        $method->setAccessible(true);

        return $method->invoke($browser);
    };

    // Bold, and in the color of a pending deletion rather than dim.
    expect($painted())->toContain("\e[1m");

    // The next key press settles it, whatever that key was.
    $browser->emit('key', 'j');

    expect($browser->statusFresh)->toBeFalse();
});

it('does not relight a status that says the same thing', function () {
    $browser = jsonBrowser();

    $browser->emit('key', 'y');
    $first = $browser->status;

    $browser->emit('key', 'y');

    expect($browser->status)->toBe($first)
        ->and($browser->statusFresh)->toBeFalse();
});

it('fits every line of help without cutting it, and names the arrow keys', function (int $columns) {
    putenv("COLUMNS={$columns}");

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', '?');

    $frame = frameOf($browser);
    $island = $browser->helpIsland;

    putenv('COLUMNS');

    $inside = collect(explode("\n", $frame))
        ->slice($island->y, $island->innerHeight())
        ->map(fn (string $line) => mb_substr($line, $island->x, $island->innerWidth()));

    expect($inside->filter(fn (string $line) => str_contains($line, '…'))->all())->toBe([])
        ->and($inside->implode("\n"))->not->toContain('OA')
        ->and($inside->implode("\n"))->toContain('←↓↑→ / hjkl  move')
        ->and($inside->implode("\n"))->toContain('dbl click    edit the cell');
})->with([80, 130]);

it('keeps a long pane title inside its own border', function () {
    $browser = browserFor(sqliteFixture());
    $browser->filter = 'widgets_and_everything_else_that_matches';

    $lines = explode("\n", frameOf($browser));
    $top = collect($lines)->first(fn (string $line) => str_starts_with($line, '┌─ '));
    $body = collect($lines)->first(fn (string $line) => str_starts_with($line, '│'));

    expect(mb_strpos($top, '┐'))->toBe(mb_strpos($body, '│', 1))
        ->and($top)->toContain('…');
});

it('titles the table list with the database it is showing', function () {
    $path = sqliteFixture();
    $browser = browserFor($path);

    $top = collect(explode("\n", frameOf($browser)))->first(fn (string $line) => str_starts_with($line, '┌─ '));

    expect($top)->toContain(mb_substr(basename($path), 0, 12))
        ->and($top)->not->toContain('TABLES');

    $style = new Styler(fn ($t) => $t, fn ($t) => $t, fn ($t) => $t, fn ($t) => $t, fn (string $t, int $w) => mb_strlen($t) > $w ? mb_substr($t, 0, $w - 1).'…' : $t);

    expect((new SidebarIsland([], 0, $style))->heading('wordpress', 22))->toBe('wordpress')
        ->and((new SidebarIsland([], 0, $style))->heading('', 22))->toBe('TABLES')
        ->and((new SidebarIsland([], 0, $style, 'options'))->heading('wordpress', 22))->toBe('wordpress /options')
        ->and((new SidebarIsland([], 0, $style, 'options'))->heading('a_database_with_a_long_name', 22))->toBe('a_database_w… /options');
});

it('jumps to a pane by its number, and titles each pane with it', function () {
    config(['tql.ui.sql_always' => true]);

    $browser = browserFor(sqliteFixture());

    $frame = frameOf($browser);

    expect($frame)->toMatch('/┌─ \[1\] /')
        ->and($frame)->toContain('─ [2] widgets ')
        ->and($frame)->toContain('─ [3] SQL ');

    $browser->emit('key', "\e1");
    expect($browser->focus)->toBe('sidebar');

    $browser->emit('key', "\e3");
    expect($browser->mode)->toBe('query');

    $browser->emit('key', "\e");
    $browser->emit('key', "\e2");
    expect($browser->focus)->toBe('grid')
        ->and($browser->mode)->toBe('browse');

    config(['tql.ui.sql_always' => false]);
});

it('opens the SQL editor with 3 even when it is hidden', function () {
    $browser = browserFor(sqliteFixture());

    $browser->emit('key', "\e3");

    expect($browser->mode)->toBe('query');
});

it('hides the table list with a backslash and gives the grid the width', function () {
    $browser = browserFor(sqliteFixture());
    $browser->emit('key', "\e1");

    $browser->emit('key', '\\');

    $frame = frameOf($browser);

    expect($browser->tablesHidden)->toBeTrue()
        ->and($browser->focus)->toBe('grid')
        ->and($browser->table->x)->toBe(1)
        ->and($frame)->not->toMatch('/┌─ \[1\] /')
        ->and($browser->status)->toContain('\\ shows it');

    $browser->emit('key', "\t");
    expect($browser->focus)->toBe('grid');

    $browser->emit('key', '\\');
    frameOf($browser);

    expect($browser->tablesHidden)->toBeFalse()
        ->and($browser->table->x)->toBeGreaterThan(1);
});

it('brings the table list back with 1, or with / to filter it', function () {
    $browser = browserFor(sqliteFixture());
    $browser->emit('key', '\\');

    $browser->emit('key', "\e1");

    expect($browser->tablesHidden)->toBeFalse()
        ->and($browser->focus)->toBe('sidebar');

    $browser->emit('key', '\\');
    $browser->emit('key', '/');

    expect($browser->tablesHidden)->toBeFalse();
});

it('resizes the table list from inside it, and a column from the grid', function () {
    $browser = browserFor(sqliteFixture());
    $start = $browser->tablesWidth();

    $browser->emit('key', "\e1");
    $browser->emit('key', '>');
    frameOf($browser);

    expect($browser->tablesWidth())->toBe($start + 4)
        ->and($browser->sidebar->width)->toBe($start + 6)
        ->and($browser->widthOverrides)->toBe([]);

    $browser->emit('key', '<');
    $browser->emit('key', '<');
    expect($browser->tablesWidth())->toBe($start - 4);

    $browser->emit('key', '=');
    expect($browser->tablesWidth())->toBe($start);

    $browser->emit('key', "\e2");
    $browser->emit('key', '>');

    expect($browser->tablesWidth())->toBe($start)
        ->and($browser->widthOverrides)->not->toBe([]);
});

it('resizes the table list by dragging its border', function () {
    config(['tql.ui.mouse_row_offset' => 0]);
    Config::set('tql.ui.mouse', true);

    $browser = browserFor(sqliteFixture());
    $border = $browser->sidebar->x + $browser->sidebar->width - 1;
    $row = $browser->sidebar->y + 3;

    $browser->emit('key', "\e[<0;{$border};{$row}M");
    $browser->emit('key', "\e[<32;".($border + 10).";{$row}M");
    $browser->emit('key', "\e[<0;".($border + 10).";{$row}m");

    expect($browser->tablesWidth())->toBe($border + 8);
});

it('goes back to the table list from the first column with left or h', function (string $key) {
    $browser = browserFor(sqliteFixture());
    $browser->emit('key', "\e2");
    $browser->emit('key', 'l');

    $browser->emit('key', $key);

    expect($browser->focus)->toBe('grid')
        ->and($browser->columnIndex)->toBe(0);

    $browser->emit('key', '\\');
    $browser->emit('key', $key);

    expect($browser->focus)->toBe('sidebar')
        ->and($browser->tablesHidden)->toBeFalse();

    $browser->emit('key', $key);

    expect($browser->focus)->toBe('sidebar');
})->with([Key::LEFT_ARROW, 'h']);

it('fits the hotkey bar on one line, keeping More and Help, at any width', function (int $columns) {
    putenv("COLUMNS={$columns}");

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', "\e2");
    $browser->emit('key', 'd');

    $lines = array_values(array_filter(explode("\n", frameOf($browser)), fn (string $line) => trim($line) !== ''));
    $bar = $lines[count($lines) - 2];

    putenv('COLUMNS');

    expect($bar)->toEndWith('ctrl+k More   ? Help')
        ->and($bar)->toStartWith(' :w Write')
        ->and(mb_strlen($bar))->toBeLessThanOrEqual($columns)
        ->and($lines[count($lines) - 3])->not->toContain('Edit');
})->with([60, 80, 100]);

it('calls out unwritten changes on the right of the status line', function (int $columns) {
    putenv("COLUMNS={$columns}");

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', "\e2");
    $browser->emit('key', 'd');

    $lines = array_values(array_filter(explode("\n", frameOf($browser)), fn (string $line) => trim($line) !== ''));
    $status = end($lines);

    putenv('COLUMNS');

    expect($status)->toEndWith('1 marked for deletion · :w writes · u clears ')
        ->and(substr_count($status, ':w writes'))->toBe(1)
        ->and(mb_strlen($status))->toBeLessThanOrEqual($columns)
        ->and(ltrim($status))->not->toStartWith('1 marked');
})->with([60, 100]);

it('puts what tql just said on the right of the status line, before any pending callout', function () {
    putenv('COLUMNS=100');

    $browser = browserFor(sqliteFixture(), 'shop');
    $status = function () use ($browser) {
        $lines = array_values(array_filter(explode("\n", frameOf($browser)), fn (string $line) => trim($line) !== ''));

        return end($lines);
    };

    $browser->status = 'pending changes dropped';

    expect($status())->toEndWith('pending changes dropped ')
        ->and(ltrim($status()))->toStartWith('shop');

    $browser->emit('key', "\e2");
    $browser->emit('key', 'd');
    $browser->status = 'copied the value';

    putenv('COLUMNS');

    expect($status())->toEndWith('copied the value · 1 marked for deletion · :w writes · u clears ');
});

it('fades a status message after a few seconds, but keeps the pending callout', function () {
    config(['tql.ui.status_seconds' => 4]);

    $browser = browserFor(sqliteFixture());
    $browser->emit('key', "\e2");
    $browser->emit('key', 'd');
    $browser->status = 'copied the value';

    expect($browser->fadeStatus($browser->statusSince + 3))->toBeFalse()
        ->and($browser->status)->toBe('copied the value');

    expect($browser->fadeStatus($browser->statusSince + 5))->toBeTrue()
        ->and($browser->status)->toBeNull()
        ->and(frameOf($browser))->toContain('1 marked for deletion · :w writes · u clears')
        ->and(frameOf($browser))->not->toContain('copied the value');
});

it('keeps a status message when status_seconds is 0', function () {
    config(['tql.ui.status_seconds' => 0]);

    $browser = browserFor(sqliteFixture());
    $browser->status = 'copied the value';

    expect($browser->fadeStatus($browser->statusSince + 3600))->toBeFalse()
        ->and($browser->status)->toBe('copied the value');

    config(['tql.ui.status_seconds' => 4]);
});

it('asks for a bigger window rather than wrapping a layout that cannot fit', function (int $columns, int $rows) {
    putenv("COLUMNS={$columns}");
    putenv("LINES={$rows}");

    $browser = browserFor(sqliteFixture());
    $lines = array_values(array_filter(explode("\n", frameOf($browser)), fn (string $line) => $line !== ''));

    putenv('COLUMNS');
    putenv('LINES');

    expect(implode("\n", $lines))->toContain(mb_substr('Make the window bigger', 0, $columns))
        ->and(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual($columns)
        ->and(count($lines))->toBeLessThan($rows)
        ->and(implode("\n", $lines))->not->toContain('┌');
})->with([[40, 30], [80, 8], [12, 4]]);

it('draws the full layout from the smallest size it fits', function () {
    putenv('COLUMNS=60');
    putenv('LINES=12');

    $frame = frameOf(browserFor(sqliteFixture()));

    putenv('COLUMNS');
    putenv('LINES');

    expect($frame)->toContain('┌')
        ->and($frame)->not->toContain('Make the window bigger');
});
