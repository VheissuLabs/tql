<?php

use App\Database\QueryRunner;
use App\Keys\Keymap;
use App\Keys\Keys;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;
use Laravel\Prompts\Key;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.keys' => [], 'tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
    Keymap::forget();
});

afterEach(function () {
    config(['tql.keys' => []]);
    Keymap::forget();
});

function bound(): Browser
{
    $path = sys_get_temp_dir().'/tql-keys-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table widgets (id integer primary key, name text)');
    $pdo->exec("insert into widgets (name) values ('alpha'), ('beta')");

    $connection = Connection::create([
        'name' => 'keys'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

it('reads a name a person would write for a key', function () {
    expect(Keys::bytes('f'))->toBe('f')
        ->and(Keys::bytes('N'))->toBe('N')
        ->and(Keys::bytes('ctrl+o'))->toBe("\x0f")
        ->and(Keys::bytes('tab'))->toBe(Key::TAB)
        ->and(Keys::bytes('escape'))->toBe(Key::ESCAPE)
        ->and(Keys::bytes('space'))->toBe(' ')
        // Nothing sensible to make of it.
        ->and(Keys::bytes('mash'))->toBeNull()
        ->and(Keys::bytes(''))->toBeNull();
});

it('spells a key the way it was written', function () {
    expect(Keys::spell('f'))->toBe('f')
        ->and(Keys::spell("\x0f"))->toBe('ctrl+o')
        ->and(Keys::spell(Key::TAB))->toBe('tab');
});

it('binds an action to the key the config asks for', function () {
    config(['tql.keys' => ['filter_rows' => 'F']]);
    Keymap::forget();

    expect(Keymap::action('F'))->toBe('filter_rows')
        ->and(Keymap::action('f'))->toBeNull()
        ->and(Keymap::key('filter_rows'))->toBe('F');
});

it('drives the grid with the rebound key', function () {
    config(['tql.keys' => ['filter_rows' => 'F', 'new_row' => 'ctrl+n']]);
    Keymap::forget();

    $browser = bound();

    $browser->emit('key', 'f');

    expect($browser->filterForm)->toBeNull();

    $browser->emit('key', 'F');

    expect($browser->filterForm)->not->toBeNull();

    $browser->emit('key', "\e");
    $browser->emit('key', "\e");
    $browser->emit('key', "\x0e");

    expect($browser->recordForm)->not->toBeNull();
});

it('shows the rebound key in help and on the hotkey bar', function () {
    config(['tql.keys' => ['filter_rows' => 'F']]);
    Keymap::forget();

    $browser = bound();

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $plain = fn () => preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    $plain();
    $browser->emit('key', "\n");

    expect($plain())->toContain('F Filter');

    $browser->emit('key', '?');

    expect($plain())->toContain('F ');
});

it('leaves movement and the keys a terminal names alone', function () {
    config(['tql.keys' => ['move_up' => 'w', 'activate' => 'x']]);
    Keymap::forget();

    expect(Keymap::action('w'))->toBeNull()
        ->and(Keymap::action(Key::ENTER))->toBe('activate');
});

it('says when two actions ask for the same key', function () {
    config(['tql.keys' => ['filter_rows' => 'r']]);
    Keymap::forget();

    expect(Keymap::clashes())->not->toBeEmpty();

    config(['tql.keys' => []]);
    Keymap::forget();

    expect(Keymap::clashes())->toBe([]);
});

it('ignores a key it cannot make sense of', function () {
    config(['tql.keys' => ['filter_rows' => 'nonsense']]);
    Keymap::forget();

    expect(Keymap::action('f'))->toBe('filter_rows');
});

it('takes several keys for one action', function () {
    config(['tql.keys' => ['yank_value' => ['y', 'ctrl+y']]]);
    Keymap::forget();

    // Either key does it; the first is the one help and the hotkey bar show.
    expect(Keymap::action('y'))->toBe('yank_value')
        ->and(Keymap::action("\x19"))->toBe('yank_value')
        ->and(Keymap::key('yank_value'))->toBe('y')
        ->and(Keymap::binding('yank_value')->shown())->toBe('y / ctrl+y');

    $browser = bound();

    $browser->emit('key', "\x19");

    expect($browser->status)->toContain('yanked');
});
