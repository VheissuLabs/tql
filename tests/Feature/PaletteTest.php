<?php

use App\Database\QueryRunner;
use App\Keys\Keymap;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\Palette;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false, 'tql.keys' => []]);
    Keymap::forget();
});

afterEach(function () {
    config(['tql.keys' => []]);
    Keymap::forget();
});

function paletted(): Browser
{
    $path = sys_get_temp_dir().'/tql-palette-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table invoices (id integer primary key, total numeric)');
    $pdo->exec('create table customers (id integer primary key, name text)');
    $pdo->exec("insert into customers (name) values ('Ada')");

    $connection = Connection::create(['name' => 'palette'.uniqid(), 'driver' => 'sqlite', 'database' => $path]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    paletteFrame($browser);

    return $browser;
}

function paletteFrame(Browser $browser): string
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));
}

function paletteType(Browser $browser, string $text): void
{
    foreach (mb_str_split($text) as $char) {
        $browser->emit('key', $char);
    }
}

it('opens with ctrl+k and closes with escape', function () {
    $browser = paletted();

    $browser->emit('key', "\x0b");

    expect($browser->palette)->not->toBeNull()
        ->and(paletteFrame($browser))->toContain('COMMANDS');

    $browser->emit('key', "\e");

    expect($browser->palette)->toBeNull();
});

it('lists actions with the key that does each one', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");

    $items = collect($browser->palette->items());
    $filter = $items->firstWhere('target', 'filter_rows');

    expect($filter['label'])->toBe('Filter rows')
        ->and($filter['hint'])->toBe('f')
        ->and($items->pluck('target'))->not->toContain('move_up')
        ->and($items->pluck('target'))->not->toContain('palette');

    paletteType($browser, 'filter');

    expect(paletteFrame($browser))->toMatch('/Filter rows\s+f /');
});

it('lists the tables, the commands without a key, and the other connections', function () {
    Connection::create(['name' => 'elsewhere', 'driver' => 'sqlite', 'database' => '/tmp/elsewhere.sqlite']);

    $browser = paletted();
    $browser->emit('key', "\x0b");

    $items = collect($browser->palette->items());

    expect($items->where('kind', Palette::TABLE)->pluck('label')->all())->toBe(['customers', 'invoices'])
        ->and($items->firstWhere('target', 'w')['hint'])->toBe(':w')
        ->and($items->where('kind', Palette::CONNECTION)->pluck('label')->all())->toBe(['elsewhere']);
});

it('ranks an exact match, then a prefix, then a word, then letters in order', function () {
    expect(Palette::score('filter rows', 'filter rows'))->toBe(0)
        ->and(Palette::score('filter rows', 'filt'))->toBe(1)
        ->and(Palette::score('filter rows', 'rows'))->toBe(2)
        ->and(Palette::score('customers', 'tom'))->toBe(3)
        ->and(Palette::score('filter rows', 'frw'))->toBe(4)
        ->and(Palette::score('filter rows', 'xyz'))->toBeNull();

    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'filt');

    expect(array_slice(array_column($browser->palette->matches(), 'target'), 0, 2))->toBe(['filter_rows', 'filter_tables']);
});

it('types j and k rather than moving', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'jk');

    expect($browser->palette->query->buffer())->toBe('jk');
});

it('runs an action the same way its key does', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'filter rows');
    $browser->emit('key', "\n");

    expect($browser->palette)->toBeNull()
        ->and($browser->filterForm)->not->toBeNull();
});

it('runs a command', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'write the pending');
    $browser->emit('key', "\n");

    expect($browser->status)->toBe('nothing to write');
});

it('goes to a table', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'invoices');
    $browser->emit('key', "\n");

    expect($browser->currentTable())->toBe('invoices')
        ->and($browser->focus)->toBe('grid');
});

it('switches to another connection', function () {
    $other = Connection::create(['name' => 'elsewhere', 'driver' => 'sqlite', 'database' => '/tmp/elsewhere.sqlite']);

    $browser = paletted();
    $browser->emit('key', "\x0b");
    paletteType($browser, 'elsewhere');
    $browser->emit('key', "\n");

    expect($browser->value())->toBe('open:'.$other->id);
});

it('moves with the arrows and ctrl+n and ctrl+p', function () {
    $browser = paletted();
    $browser->emit('key', "\x0b");

    $browser->emit('key', "\e[B");
    $browser->emit('key', "\x0e");

    expect($browser->palette->index)->toBe(2);

    $browser->emit('key', "\x10");

    expect($browser->palette->index)->toBe(1);
});

it('opens on the key the config gives it, and shows rebound keys', function () {
    config(['tql.keys' => ['palette' => 'ctrl+p', 'filter_rows' => 'F']]);
    Keymap::forget();

    $browser = paletted();

    $browser->emit('key', "\x0b");

    expect($browser->palette)->toBeNull();

    $browser->emit('key', "\x10");

    expect($browser->palette)->not->toBeNull()
        ->and(collect($browser->palette->items())->firstWhere('target', 'filter_rows')['hint'])->toBe('F');
});

it('opens from the SQL editor too', function () {
    $browser = paletted();
    $browser->emit('key', 's');

    expect($browser->mode)->toBe('query');

    $browser->emit('key', "\x0b");

    expect($browser->palette)->not->toBeNull();
});

it('fits inside an 80 column terminal', function () {
    putenv('COLUMNS=80');

    $browser = paletted();
    $browser->emit('key', "\x0b");

    $lines = explode("\n", paletteFrame($browser));

    putenv('COLUMNS');

    expect(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual(80);
});
