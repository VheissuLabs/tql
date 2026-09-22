<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
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
