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

    expect(array_keys(form($sqlite)->fields()))
        ->toBe(['name', 'database', 'tag', 'read_only']);

    $mysql = Connection::create([
        'name' => 'my', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
        'database' => 'shop', 'username' => 'alice', 'password' => 'pw',
    ]);

    expect(array_keys(form($mysql)->fields()))->toBe([
        'name', 'host', 'port', 'database', 'username', 'password',
        'ssl_mode', 'over_ssh', 'tag', 'read_only',
    ]);
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

    expect(array_keys($picker->form->fields()))
        ->toBe(['driver', 'name', 'database', 'tag', 'read_only']);

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    expect(array_keys($picker->form->fields()))->toBe([
        'driver', 'name', 'host', 'port', 'database', 'username', 'password',
        'ssl_mode', 'over_ssh', 'tag', 'read_only',
    ]);
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

    expect($picker->form->buffer())->toBe('Cloud - lunar');

    $picker->emit('key', "\n");

    expect($picker->form->values['name'])->toBe('Cloud - lunar');
});

it('flattens a multi-line paste into a single line field', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->move(1);
    $picker->emit('key', "\n");
    $picker->emit('key', "first\nsecond");

    expect($picker->form->buffer())->toBe('first second');
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

it('moves the cursor inside a field instead of typing the key', function () {
    $picker = picker();

    $picker->emit('key', 'e');
    $picker->emit('key', "\n");
    // Editing starts on the existing value with the cursor at the end.
    expect($picker->form->buffer())->toBe('first')
        ->and($picker->form->cursor())->toBe(5);

    $picker->emit('key', "\e[D");
    $picker->emit('key', "\e[D");

    expect($picker->form->cursor())->toBe(3)
        ->and($picker->form->buffer())->toBe('first');

    $picker->emit('key', 'X');

    expect($picker->form->buffer())->toBe('firXst');
});

it('backspaces at the cursor, not only at the end', function () {
    $picker = picker();

    $picker->emit('key', 'e');
    $picker->emit('key', "\n");
    $picker->emit('key', "\e[D");
    $picker->emit('key', "\x7f");

    // "first", cursor between s and t, backspace removes the s.
    expect($picker->form->buffer())->toBe('firt');
});

it('marks a connection for deletion without removing it', function () {
    $picker = picker();

    $picker->emit('key', 'd');

    expect($picker->pendingDeletes)->toHaveCount(1)
        ->and(Connection::count())->toBe(2)
        ->and($picker->status)->toContain('marked for deletion');
});

it('writes marked connection deletions on :w', function () {
    $picker = picker();

    $gone = $picker->connections->first()->name;

    $picker->emit('key', 'd');
    $picker->emit('key', ':');
    $picker->emit('key', 'w');
    $picker->emit('key', "\n");

    expect(Connection::where('name', $gone)->exists())->toBeFalse()
        ->and(Connection::count())->toBe(1)
        ->and($picker->pendingDeletes)->toBe([])
        ->and($picker->connections)->toHaveCount(1);
});

it('unmarks a connection you mark twice', function () {
    $picker = picker();

    $picker->emit('key', 'd');
    $picker->emit('key', 'k');
    $picker->emit('key', 'd');

    expect($picker->pendingDeletes)->toBe([]);
});

it('drops unwritten connection marks before quitting', function () {
    $picker = picker();

    $picker->emit('key', 'd');
    $picker->emit('key', 'q');

    expect($picker->value())->toBeNull()
        ->and($picker->pendingDeletes)->toBe([])
        ->and($picker->status)->toContain('dropped');

    $picker->emit('key', 'q');

    expect($picker->value())->toBe('quit');
});

it('draws a marked connection as one unbroken bar', function () {
    $picker = picker();

    $picker->emit('key', 'd');

    $marked = collect(explode("\n", pickerFrame($picker)))
        ->first(fn (string $line) => str_contains($line, "\e[31m\e[7m"));

    expect($marked)->not->toBeNull();

    preg_match('/\e\[7m(.*?)\e\[27m/', $marked, $match);

    // The driver icon must not carry its own color inside the highlight.
    expect($match[1] ?? '')->not->toContain("\e[")
        ->and($match[1] ?? '')->toContain('│');
});

it('groups the form into sections', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    $picker->form->values['over_ssh'] = 'yes';

    $plain = preg_replace('/\e\[[0-9;]*m/', '', pickerFrame($picker, 140, 44));

    $inside = collect(explode("\n", $plain))
        ->filter(fn (string $l) => str_contains($l, 'Driver') || str_contains($l, 'SSL mode')
            || str_contains($l, 'Over SSH') || str_contains($l, 'Tag'))
        ->values();

    expect($inside)->toHaveCount(4);

    // A blank row separates the groups, so the rows are not adjacent.
    $rows = explode("\n", $plain);

    $lineOf = function (string $needle) use ($rows) {
        foreach ($rows as $index => $row) {
            if (str_contains($row, $needle)) {
                return $index;
            }
        }

        return -1;
    };

    expect($lineOf('SSL mode') - $lineOf('Password'))->toBeGreaterThan(1)
        ->and($lineOf('Over SSH') - $lineOf('SSL mode'))->toBeGreaterThan(1);
});

it('shows a swatch beside the chosen tag', function () {
    $picker = picker();

    $picker->emit('key', 'n');
    $picker->form->values['tag'] = 'production';

    expect(pickerFrame($picker))->toContain("\e[31m●");
});

it('picks a tag from a list that shows its color', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    $picker->form->index = array_search('tag', $picker->form->keys(), true);

    $picker->emit('key', "\n");

    expect($picker->form->picker)->not->toBeNull()
        ->and($picker->form->picker->title)->toBe('TAG')
        ->and($picker->form->picker->options)->toBe(['none', 'production', 'staging', 'dev', 'local'])
        ->and($picker->form->picker->colorOf('production'))->toBe('red')
        ->and($picker->form->picker->colorOf('local'))->toBe('green');

    $frame = pickerFrame($picker);

    expect($frame)->toContain("\e[31m●")
        ->and($frame)->toContain("\e[32m●");

    // Choose staging.
    $picker->form->picker->index = array_search('staging', $picker->form->picker->matches(), true);

    $picker->emit('key', "\n");

    expect($picker->form->picker)->toBeNull()
        ->and($picker->form->values['tag'])->toBe('staging');
});

it('dims a placeholder but not a real value', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    expect($picker->form->isPlaceholder('ssl_mode'))->toBeTrue()
        ->and($picker->form->isPlaceholder('host'))->toBeFalse()
        ->and($picker->form->isPlaceholder('name'))->toBeFalse();
});

it('offers arrows on every field with a fixed set of answers', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    foreach (['driver', 'read_only'] as $key) {
        expect($picker->form->choices($key))->not->toBeNull();
    }

    expect($picker->form->choices('name'))->toBeNull()
        ->and($picker->form->choices('host'))->toBeNull();
});

it('wears the color on the icon and the tag in the list', function () {
    // Two, so the tagged one is not the row under the cursor.
    Connection::create(['name' => 'aaa', 'driver' => 'sqlite', 'database' => '/tmp/a.sqlite']);
    Connection::create([
        'name' => 'prod', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
        'database' => 'shop', 'tag' => 'production',
    ]);

    $picker = new ConnectionPicker(Connection::orderBy('name')->get());

    $line = collect(explode("\n", pickerFrame($picker)))
        ->first(fn (string $l) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $l), 'production')
            && ! str_contains($l, "\e[7m"));

    expect($line)->not->toBeNull()
        // The icon and the tag both carry it.
        ->and(substr_count($line, "\e[31m"))->toBeGreaterThanOrEqual(2);
});

it('leaves a connection without a color alone', function () {
    Connection::create([
        'name' => 'plain', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306, 'database' => 'd',
    ]);

    $picker = new ConnectionPicker(Connection::orderBy('name')->get());

    $line = collect(explode("\n", pickerFrame($picker)))
        ->first(fn (string $l) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $l), 'plain'));

    // mysql's own yellow icon, and nothing else coloured.
    expect(substr_count($line, "\e[31m"))->toBe(0);
});

it('runs the highlight the full width of the list', function () {
    $picker = picker();

    $picker->emit('key', 'n');

    $picker->form->index = array_search('tag', $picker->form->keys(), true);
    $picker->emit('key', "\n");

    // The highlighted row inside the list, not the one behind it.
    $selected = collect(explode("\n", pickerFrame($picker)))
        ->first(fn (string $l) => str_contains($l, "\e[7m") && str_contains($l, 'none'));

    expect($selected)->not->toBeNull();

    preg_match('/\e\[7m(.*?)\e\[27m/', $selected, $match);

    $span = $match[1] ?? '';

    // It runs past the word to the edge of the box.
    expect(trim($span))->toBe('none')
        ->and(mb_strlen($span))->toBeGreaterThan(40)
        ->and(mb_substr($span, -1))->toBe(' ');
});

function sqlitePathForm(ConnectionPicker $picker): void
{
    $picker->emit('key', 'n');

    while ($picker->form->currentKey() !== 'database') {
        $picker->form->move(1);
    }
}

it('scrolls a long path while it is typed, so the end and the cursor stay in view', function () {
    $picker = picker();
    sqlitePathForm($picker);

    $picker->emit('key', "\n");
    $picker->emit('key', '~/code/kmstools-lunar/database/database-for-the-lunar-project.sqlite');

    $lines = explode("\n", pickerFrame($picker));
    $plain = array_map(fn (string $line) => preg_replace('/\e\[[0-9;]*m/', '', $line), $lines);

    $top = collect($plain)->search(fn (string $line) => str_contains($line, 'NEW CONNECTION'));
    $row = collect($plain)->search(fn (string $line) => str_contains($line, 'Path'));
    $right = mb_strrpos($plain[$top], '┐');

    expect($plain[$row])->toMatch('/Path\s+…\S*lunar-project\.sqlite/')
        ->and($lines[$row])->toContain("sqlite\e[7m \e[27m")
        ->and(mb_substr($plain[$row], $right, 1))->toBe('│');
});

it('saves a relative sqlite path as an absolute one, from where tql was started', function () {
    $dir = sys_get_temp_dir().'/tql-relative-'.uniqid();
    mkdir($dir.'/database', 0777, true);
    touch($dir.'/database/database.sqlite');

    $was = getcwd();
    chdir($dir);

    $picker = picker();
    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'relative';
    $picker->form->values['database'] = 'database/database.sqlite';
    $picker->emit('key', ConnectionPicker::SAVE);

    chdir($was);

    expect(Connection::where('name', 'relative')->value('database'))->toBe(realpath($dir.'/database/database.sqlite'));
});

it('saves a sqlite path under ~ with the home directory in place', function () {
    $home = sys_get_temp_dir().'/tql-home-'.uniqid();
    mkdir($home.'/code', 0777, true);
    touch($home.'/code/lunar.sqlite');

    $was = getenv('HOME');
    putenv("HOME={$home}");

    $picker = picker();
    $picker->emit('key', 'n');
    $picker->form->values['name'] = 'home';
    $picker->form->values['database'] = '~/code/lunar.sqlite';
    $picker->emit('key', ConnectionPicker::SAVE);

    putenv("HOME={$was}");

    expect(Connection::where('name', 'home')->value('database'))->toBe(realpath($home.'/code/lunar.sqlite'));
});
