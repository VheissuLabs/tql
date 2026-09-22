<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use App\Tui\Theme;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['dotsql.theme' => ['border' => 'dim', 'focus_border' => 'cyan', 'focus_title' => 'cyan']]);
    config(['dotsql.ui.mouse_row_offset' => 0]);
});

function themed(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-theme-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table t (id integer primary key, name text, payload text)');
    $pdo->exec('insert into t (name, payload) values (\'one\', \'{"a":1}\')');
    $pdo->exec('insert into t (name, payload) values (\'two\', \'{"b":2}\')');

    $connection = Connection::create([
        'name' => 'theme'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

function frameFor(Browser $browser): string
{
    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    return $method->invoke($browser);
}

it('paints the focused border with the configured colour', function (string $colour, string $code) {
    config(['dotsql.theme.focus_border' => $colour, 'dotsql.theme.focus_title' => $colour]);

    expect(frameFor(themed()))->toContain("\e[{$code}m");
})->with([
    ['cyan', '36'],
    ['magenta', '35'],
    ['green', '32'],
    ['yellow', '33'],
]);

it('falls back to dim for a colour it does not know', function () {
    config(['dotsql.theme.focus_border' => 'ultraviolet']);

    expect(Theme::border(true))->toBe('dim');
});

it('uses a different colour for focused and unfocused panes', function () {
    config(['dotsql.theme.focus_border' => 'green', 'dotsql.theme.border' => 'red']);

    $frame = frameFor(themed());

    expect($frame)->toContain("\e[32m")
        ->and($frame)->toContain("\e[31m");
});

it('moves the focus colour when the focus moves', function () {
    config(['dotsql.theme.focus_border' => 'green', 'dotsql.theme.border' => 'dim']);

    $browser = themed();

    $sidebarEdge = function (Browser $browser) {
        foreach (explode("\n", frameFor($browser)) as $line) {
            if (str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 'TABLES')) {
                return str_starts_with($line, "\e[32m");
            }
        }

        return false;
    };

    expect($browser->focus)->toBe('sidebar')
        ->and($sidebarEdge($browser))->toBeTrue();

    $browser->emit('key', "\t");

    expect($browser->focus)->toBe('grid')
        ->and($sidebarEdge($browser))->toBeFalse();
});

it('does not quit the browser on escape', function () {
    $browser = themed();

    $browser->emit('key', "\e");

    expect($browser->state)->not->toBe('submit');
});

it('cycles panes forward with tab and backward with shift+tab', function () {
    config(['dotsql.ui.sql_always' => false]);

    $browser = themed();

    expect($browser->focus)->toBe('sidebar');

    $browser->emit('key', "\t");
    expect($browser->focus)->toBe('grid');

    $browser->emit('key', "\e[Z");
    expect($browser->focus)->toBe('sidebar');

    $browser->emit('key', "\e[Z");
    expect($browser->focus)->toBe('grid');
});

it('includes the sql pane in the cycle when it is always shown', function () {
    config(['dotsql.ui.sql_always' => true]);

    $browser = themed();

    $browser->emit('key', "\e[Z");

    expect($browser->mode)->toBe('query');

    $browser->emit('key', "\e[Z");

    expect($browser->mode)->toBe('browse')
        ->and($browser->focus)->toBe('grid');

    $browser->emit('key', "\t");

    expect($browser->mode)->toBe('query');
});

it('keeps the frame colour off the column ticks and the interior grid', function () {
    config([
        'dotsql.theme.focus_border' => 'blue',
        'dotsql.theme.focus_title' => 'blue',
        'dotsql.theme.grid' => 'gray',
    ]);

    $browser = themed();
    $browser->emit('key', "\n");

    $frame = frameFor($browser);

    // Every column tick on a border row is grid-coloured, never frame-coloured.
    foreach (explode("\n", $frame) as $line) {
        foreach (['┬', '┴'] as $tick) {
            if (! str_contains($line, $tick)) {
                continue;
            }

            expect($line)->toContain("\e[90m".$tick);
        }
    }

    // And the frame colour never immediately precedes a tick.
    expect($frame)->not->toContain("\e[34m┬")
        ->and($frame)->not->toContain("\e[34m┴")
        ->and($frame)->not->toContain("\e[34m│ ");
});

it('paints the interior grid separately from the border', function () {
    config(['dotsql.theme.grid' => 'red', 'dotsql.theme.focus_border' => 'blue']);

    $browser = themed();
    $browser->emit('key', "\n");

    $frame = frameFor($browser);

    expect($frame)->toContain("\e[31m│");

    $rule = collect(explode("\n", $frame))->first(fn (string $line) => str_contains($line, '┼'));

    expect($rule)->not->toBeNull()
        ->and($rule)->toMatch('/\e\[31m[─┼]*┼/');
});
