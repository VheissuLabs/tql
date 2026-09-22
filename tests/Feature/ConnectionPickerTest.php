<?php

use App\Models\Connection;
use App\Tui\ConnectionForm;
use App\Tui\ConnectionPicker;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();
});

function picker(): ConnectionPicker
{
    Connection::create(['name' => 'first', 'driver' => 'sqlite', 'database' => '/tmp/a.sqlite']);
    Connection::create(['name' => 'second', 'driver' => 'sqlite', 'database' => '/tmp/b.sqlite']);

    return new ConnectionPicker(Connection::orderBy('name')->get());
}

it('edits the highlighted connection', function () {
    $picker = picker();

    $picker->emit('key', 'e');

    expect($picker->form?->connection->name)->toBe('first');
});

it('edits the one the cursor moved to', function () {
    $picker = picker();

    $picker->emit('key', 'j');
    $picker->emit('key', 'e');

    expect($picker->form?->connection->name)->toBe('second');
});

it('still opens on enter and quits on q', function () {
    $picker = picker();

    $picker->emit('key', "\n");

    expect($picker->value())->toBe((string) Connection::orderBy('name')->first()->id);
});

it('does nothing when there is nothing to edit', function () {
    $picker = new ConnectionPicker(collect());

    $picker->emit('key', 'e');

    expect($picker->value())->toBeNull();
});

it('offers edit on the hotkey bar', function () {
    $picker = picker();

    $render = new ReflectionMethod($picker, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($picker)))->toContain('e Edit');
});

function form(Connection $connection): ConnectionForm
{
    return new ConnectionForm($connection);
}

it('opens the edit form in place rather than leaving the tui', function () {
    $picker = picker();

    $picker->emit('key', 'e');

    expect($picker->form)->not->toBeNull()
        ->and($picker->value())->toBeNull()
        ->and($picker->form->connection->name)->toBe('first');
});

it('closes the form on escape without saving', function () {
    $picker = picker();

    $picker->emit('key', 'e');
    $picker->emit('key', "\n");
    $picker->emit('key', 'X');
    $picker->emit('key', "\n");
    $picker->emit('key', "\e");

    expect($picker->form)->toBeNull()
        ->and(Connection::where('name', 'first')->exists())->toBeTrue()
        ->and($picker->status)->toBe('nothing changed');
});

it('saves on ctrl+s', function () {
    $picker = picker();

    $picker->emit('key', 'e');
    $picker->emit('key', "\n");

    foreach (str_split('!') as $char) {
        $picker->emit('key', $char);
    }

    $picker->emit('key', "\n");
    $picker->emit('key', ConnectionPicker::SAVE);

    expect($picker->form)->toBeNull()
        ->and(Connection::where('name', 'first!')->exists())->toBeTrue();
});

it('offers only the fields that driver has', function () {
    $sqlite = Connection::create(['name' => 'lite', 'driver' => 'sqlite', 'database' => '/tmp/a.sqlite']);

    expect(array_keys(form($sqlite)->fields()))->toBe(['name', 'database']);

    $mysql = Connection::create([
        'name' => 'my', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
        'database' => 'shop', 'username' => 'alice', 'password' => 'pw',
    ]);

    expect(array_keys(form($mysql)->fields()))
        ->toBe(['name', 'host', 'port', 'database', 'username', 'password']);
});

it('never shows the password back', function () {
    $connection = Connection::create([
        'name' => 'shown', 'driver' => 'mysql', 'host' => 'db.example.com', 'port' => 3306,
        'database' => 'shop', 'username' => 'alice', 'password' => 'hunter2',
    ]);

    $form = form($connection);

    expect($form->display('password'))->toBe(str_repeat('•', 7))
        ->and($form->display('password'))->not->toContain('hunter2')
        ->and($form->display('host'))->toBe('db.example.com');
});

it('shows nothing for a password that is not set', function () {
    $connection = Connection::create(['name' => 'nopw', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306]);

    expect(form($connection)->display('password'))->toBe('');
});

it('refuses to save a name that is already taken', function () {
    Connection::create(['name' => 'taken', 'driver' => 'sqlite', 'database' => '/tmp/x.sqlite']);
    $other = Connection::create(['name' => 'mine', 'driver' => 'sqlite', 'database' => '/tmp/y.sqlite']);

    $form = form($other);
    $form->values['name'] = 'taken';

    expect($form->save())->toBe('Another connection is already called that.')
        ->and($other->fresh()->name)->toBe('mine');
});

it('refuses to save an empty name', function () {
    $connection = Connection::create(['name' => 'named', 'driver' => 'sqlite', 'database' => '/tmp/z.sqlite']);

    $form = form($connection);
    $form->values['name'] = '   ';

    expect($form->save())->toBe('A name is required.');
});

it('draws the form centred over the list', function () {
    putenv('COLUMNS=120');
    putenv('LINES=30');

    $picker = picker();
    $picker->emit('key', 'e');

    $render = new ReflectionMethod($picker, 'renderTheme');
    $render->setAccessible(true);

    $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($picker)));

    putenv('COLUMNS');
    putenv('LINES');

    $modal = array_values(array_filter($lines, fn (string $l) => str_contains($l, 'EDIT CONNECTION')));

    expect($modal)->toHaveCount(1);

    // Centred: the gap before the box matches the gap after it, within a column.
    $row = $modal[0];
    $left = mb_strlen($row) - mb_strlen(ltrim(mb_substr($row, 1)));

    expect(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual(120)
        ->and(str_contains($row, 'Name'))->toBeFalse();
});

function pickerFrame(ConnectionPicker $picker, int $cols = 120, int $rows = 30): string
{
    putenv("COLUMNS={$cols}");
    putenv("LINES={$rows}");

    $render = new ReflectionMethod($picker, 'renderTheme');
    $render->setAccessible(true);

    $frame = $render->invoke($picker);

    putenv('COLUMNS');
    putenv('LINES');

    return $frame;
}

it('marks the driver with an icon instead of a column', function () {
    Connection::create(['name' => 'shop', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306, 'database' => 'd']);

    $picker = new ConnectionPicker(Connection::orderBy('name')->get());

    $plain = preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker));

    expect($plain)->toContain(config('tql.icons.mysql').' shop')
        ->and($plain)->not->toContain('DRIVER');
});

it('gives each driver its own icon', function () {
    foreach (['mysql', 'pgsql', 'sqlite', 'sqlsrv'] as $driver) {
        Connection::query()->delete();
        Connection::create(['name' => 'one', 'driver' => $driver, 'database' => '/tmp/x', 'host' => 'h', 'port' => 1]);

        $picker = new ConnectionPicker(Connection::get());

        expect(preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker)))
            ->toContain(config("tql.icons.{$driver}").' one');
    }
});

it('titles the connection frame', function () {
    expect(preg_replace('/\e\[[0-9;]*m/', '', pickerFrame(picker())))->toContain('CONNECTIONS');
});

it('keeps the connection screen inside the terminal', function () {
    $picker = picker();

    foreach ([70, 90, 120, 160, 200] as $cols) {
        $frame = pickerFrame($picker, $cols);

        foreach (explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $frame)) as $line) {
            expect(mb_strlen($line))->toBeLessThanOrEqual($cols);
        }
    }
});

it('puts the hotkeys above the status line', function () {
    $plain = preg_replace('/\e\[[0-9;]*m/', '', pickerFrame(picker()));
    $lines = array_values(array_filter(explode("\n", $plain), fn (string $l) => trim($l) !== ''));

    expect(end($lines))->toContain('connections')
        ->and(prev($lines))->toContain('Move');
});

it('draws the selected row as one unbroken bar', function () {
    $frame = pickerFrame(picker());

    $selected = collect(explode("\n", $frame))
        ->first(fn (string $line) => str_contains($line, "\e[7m"));

    // One inverse span for the whole row, not one per cell.
    expect(substr_count($selected, "\e[7m"))->toBe(1);

    preg_match('/\e\[7m(.*?)\e\[27m/', $selected, $match);

    // Nothing inside the span resets the highlight.
    expect($match[1])->not->toContain("\e[")
        ->and($match[1])->toContain('│');
});

it('shortens home paths so the database name survives', function () {
    $home = getenv('HOME');

    $connection = Connection::create([
        'name' => 'home', 'driver' => 'sqlite', 'database' => $home.'/Code/app/database.sqlite',
    ]);

    expect($connection->describe())->toBe('sqlite:~/Code/app/database.sqlite');
});

it('leaves a path outside home alone', function () {
    $connection = Connection::create([
        'name' => 'away', 'driver' => 'sqlite', 'database' => '/var/db/app.sqlite',
    ]);

    expect($connection->describe())->toBe('sqlite:/var/db/app.sqlite');
});

it('marks the current row when the marker style is on', function () {
    config(['tql.ui.row_style' => 'marker']);

    expect(preg_replace('/\e\[[0-9;]*m/', '', pickerFrame(picker())))->toContain('▸');
});

it('creates a connection in the modal rather than dropping out of the tui', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    expect($picker->form)->not->toBeNull()
        ->and($picker->form->creating)->toBeTrue()
        ->and($picker->value())->toBeNull();

    expect(preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker)))->toContain('NEW CONNECTION');
});

it('cycles the driver instead of typing it', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    expect($picker->form->currentKey())->toBe('driver')
        ->and($picker->form->driver())->toBe('sqlite');

    $picker->emit('key', "\n");

    expect($picker->form->driver())->not->toBe('sqlite')
        ->and($picker->form->editing)->toBeFalse();
});

it('shows the fields that suit the chosen driver', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    expect(array_keys($picker->form->fields()))->toBe(['driver', 'name', 'database']);

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    expect(array_keys($picker->form->fields()))
        ->toBe(['driver', 'name', 'host', 'port', 'database', 'username', 'password']);
});

it('fills in the default host and port for a server driver', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'pgsql') {
        $picker->form->cycleDriver();
    }

    expect($picker->form->values['host'])->toBe('127.0.0.1')
        ->and($picker->form->values['port'])->toBe('5432');
});

it('saves a new connection and selects it', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'brand new';
    $picker->form->values['database'] = '/tmp/brand-new.sqlite';

    $picker->emit('key', ConnectionPicker::SAVE);

    $created = Connection::where('name', 'brand new')->first();

    expect($created)->not->toBeNull()
        ->and($created->driver)->toBe('sqlite')
        ->and($picker->form)->toBeNull()
        ->and($picker->status)->toContain('added brand new')
        ->and($picker->connections->pluck('name'))->toContain('brand new');
});

it('will not save a sqlite connection with no path', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'pathless';

    $picker->emit('key', ConnectionPicker::SAVE);

    expect($picker->form)->not->toBeNull()
        ->and($picker->form->error)->toContain('path')
        ->and(Connection::where('name', 'pathless')->exists())->toBeFalse();
});

it('keeps a port you typed when the driver changes', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    $picker->form->values['port'] = '3307';

    while ($picker->form->driver() !== 'pgsql') {
        $picker->form->cycleDriver();
    }

    expect($picker->form->values['port'])->toBe('3307');
});

it('accepts a pasted value in a form field', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->move(1);

    expect($picker->form->currentKey())->toBe('name');

    $picker->emit('key', "\n");
    $picker->emit('key', 'Cloud - lunar');

    expect($picker->form->buffer)->toBe('Cloud - lunar');

    $picker->emit('key', "\n");

    expect($picker->form->values['name'])->toBe('Cloud - lunar');
});

it('flattens a multi-line paste into a single line field', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->move(1);
    $picker->emit('key', "\n");
    $picker->emit('key', "first\nsecond");

    expect($picker->form->buffer)->toBe('first second');
});

it('saves from a row you can select, not only ctrl+s', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'by enter';
    $picker->form->values['database'] = '/tmp/by-enter.sqlite';

    while (! $picker->form->onAction()) {
        $picker->emit('key', 'j');
    }

    expect($picker->form->currentKey())->toBe('save');

    $picker->emit('key', "\n");

    expect($picker->form)->toBeNull()
        ->and(Connection::where('name', 'by enter')->exists())->toBeTrue();
});

it('cancels from the cancel row', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'never saved';
    $picker->form->values['database'] = '/tmp/never.sqlite';

    while ($picker->form->currentKey() !== 'cancel') {
        $picker->emit('key', 'j');
    }

    $picker->emit('key', "\n");

    expect($picker->form)->toBeNull()
        ->and(Connection::where('name', 'never saved')->exists())->toBeFalse();
});

it('saves what you are typing without committing the field first', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['database'] = '/tmp/mid-type.sqlite';
    $picker->form->move(1);

    expect($picker->form->currentKey())->toBe('name');

    $picker->emit('key', "\n");
    $picker->emit('key', 'typed then saved');

    expect($picker->form->editing)->toBeTrue();

    $picker->emit('key', ConnectionPicker::SAVE);

    expect($picker->form)->toBeNull()
        ->and(Connection::where('name', 'typed then saved')->exists())->toBeTrue();
});

it('shows the save and cancel rows in the modal', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    $plain = preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker));

    expect($plain)->toContain('Save')
        ->and($plain)->toContain('Cancel');
});

it('does not repeat save and cancel in the hint line', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    $plain = preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker));

    expect(substr_count($plain, 'Save'))->toBe(1)
        ->and(substr_count($plain, 'Cancel'))->toBe(1)
        ->and($plain)->not->toContain('ctrl+s save')
        ->and($plain)->not->toContain('esc cancel');
});

it('says what the row under the cursor does', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    $hint = fn () => preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker));

    expect($hint())->toContain('changes the driver');

    $picker->form->move(1);

    expect($hint())->toContain('changes it');

    while (! $picker->form->onAction()) {
        $picker->form->move(1);
    }

    expect($hint())->toContain('chooses');
});
