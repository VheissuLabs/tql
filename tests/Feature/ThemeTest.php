<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use App\Tui\Theme;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.theme' => ['border' => 'dim', 'focus_border' => 'cyan', 'focus_title' => 'cyan']]);
    config(['tql.ui.mouse_row_offset' => 0]);
});

function themed(): Browser
{
    $path = sys_get_temp_dir().'/tql-theme-'.uniqid().'.sqlite';
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
    config(['tql.theme.focus_border' => $colour, 'tql.theme.focus_title' => $colour]);

    expect(frameFor(themed()))->toContain("\e[{$code}m");
})->with([
    ['cyan', '36'],
    ['magenta', '35'],
    ['green', '32'],
    ['yellow', '33'],
]);

it('falls back to dim for a colour it does not know', function () {
    config(['tql.theme.focus_border' => 'ultraviolet']);

    expect(Theme::border(true))->toBe('dim');
});

it('uses a different colour for focused and unfocused panes', function () {
    config(['tql.theme.focus_border' => 'green', 'tql.theme.border' => 'red']);

    $frame = frameFor(themed());

    expect($frame)->toContain("\e[32m")
        ->and($frame)->toContain("\e[31m");
});

it('moves the focus colour when the focus moves', function () {
    config(['tql.theme.focus_border' => 'green', 'tql.theme.border' => 'dim']);

    $browser = themed();

    $sidebarEdge = function (Browser $browser) {
        foreach (explode("\n", frameFor($browser)) as $line) {
            if (str_starts_with(preg_replace('/\e\[[0-9;]*m/', '', $line), '┌─ ')) {
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
    config(['tql.ui.sql_always' => false]);

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
    config(['tql.ui.sql_always' => true]);

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
        'tql.theme.focus_border' => 'blue',
        'tql.theme.focus_title' => 'blue',
        'tql.theme.grid' => 'gray',
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
    config(['tql.theme.grid' => 'red', 'tql.theme.focus_border' => 'blue']);

    $browser = themed();
    $browser->emit('key', "\n");

    $frame = frameFor($browser);

    expect($frame)->toContain("\e[31m│");

    $rule = collect(explode("\n", $frame))->first(fn (string $line) => str_contains($line, '┼'));

    expect($rule)->not->toBeNull()
        ->and($rule)->toMatch('/\e\[31m[─┼]*┼/');
});

it('tints the whole table with the pane colour when the grid inherits', function () {
    config([
        'tql.theme.grid' => 'inherit',
        'tql.theme.focus_border' => 'blue',
        'tql.theme.border' => 'gray',
    ]);

    $browser = themed();
    $browser->emit('key', "\n");

    expect($browser->focus)->toBe('grid');

    $frame = frameFor($browser);

    // The focused table draws its separators in the focus colour...
    expect($frame)->toContain("\e[34m│");

    // ...and the unfocused sidebar keeps the resting colour.
    $sidebar = collect(explode("\n", $frame))
        ->first(fn (string $line) => str_starts_with(preg_replace('/\e\[[0-9;]*m/', '', $line), '┌─ '));

    expect($sidebar)->toStartWith("\e[90m");
});

it('tints the cursor block with the configured colour', function () {
    config(['tql.theme.cursor' => 'magenta']);

    $browser = themed();
    $browser->emit('key', "\n");

    // The colour has to come before the inverse, or it tints the glyph
    // instead of the block behind it.
    expect(frameFor($browser))->toContain("\e[35m\e[7m");
});

it('tints the selection apart from the cursor', function () {
    config(['tql.theme.cursor' => 'magenta', 'tql.theme.selection' => 'green']);

    $frame = frameFor(themed());

    $sidebar = collect(explode("\n", $frame))
        ->first(fn (string $line) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 't  '));

    expect($sidebar)->toContain("\e[32m\e[7m")
        ->and($sidebar)->not->toContain("\e[35m");
});

it('leaves the highlight on the terminal colours by default', function () {
    config(['tql.theme.cursor' => 'default', 'tql.theme.selection' => 'default']);

    $frame = frameFor(themed());

    expect($frame)->toContain("\e[7m")
        ->and($frame)->not->toMatch('/\e\[3[0-8]m\e\[7m/');
});
