<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

function viewer(): Browser
{
    $path = sys_get_temp_dir().'/dotsql-yank-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table docs (id integer primary key, body text)');
    $pdo->prepare('insert into docs (body) values (?)')->execute([
        json_encode(['a' => 1, 'b' => 2, 'c' => ['d' => 3, 'e' => 4]]),
    ]);

    $connection = Connection::create([
        'name' => 'yank'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    return $browser;
}

it('selects a single line by default', function () {
    $browser = viewer();

    expect($browser->selectedLines())->toBe([0, 0]);

    $browser->emit('key', 'j');

    expect($browser->selectedLines())->toBe([1, 1]);
});

it('extends a selection with V and j', function () {
    $browser = viewer();

    $browser->emit('key', 'j');
    $browser->emit('key', 'V');

    expect($browser->visualAnchor)->toBe(1);

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($browser->selectedLines())->toBe([1, 3]);
});

it('selects upwards too', function () {
    $browser = viewer();

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');
    $browser->emit('key', 'j');
    $browser->emit('key', 'V');
    $browser->emit('key', 'k');
    $browser->emit('key', 'k');

    expect($browser->selectedLines())->toBe([1, 3]);
});

it('clears the selection with escape without closing', function () {
    $browser = viewer();

    $browser->emit('key', 'V');
    $browser->emit('key', 'j');

    expect($browser->visualAnchor)->not->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->visualAnchor)->toBeNull()
        ->and($browser->mode)->toBe('edit');

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse');
});

it('yanks the selected lines', function () {
    $browser = viewer();

    $browser->emit('key', 'j');
    $browser->emit('key', 'V');
    $browser->emit('key', 'j');
    $browser->emit('key', 'y');

    expect($browser->status)->toContain('yanked 2 lines')
        ->and($browser->visualAnchor)->toBeNull();
});

it('yanks the current line when nothing is selected', function () {
    $browser = viewer();

    $browser->emit('key', 'y');

    expect($browser->status)->toContain('yanked 1 line');
});

it('jumps to the top and bottom with g and G', function () {
    $browser = viewer();

    $browser->emit('key', 'G');
    $bottom = $browser->selectedLines()[0];

    $browser->emit('key', 'g');

    expect($bottom)->toBeGreaterThan(0)
        ->and($browser->selectedLines())->toBe([0, 0]);
});

it('does not yank from an editable value', function () {
    $browser = viewer();
    $browser->emit('key', "\e");
    $browser->emit('key', 'e');

    $before = $browser->cellEditor->buffer();

    $browser->emit('key', 'y');

    expect($browser->cellEditor->buffer())->toContain('y')
        ->and($browser->cellEditor->buffer())->not->toBe($before);
});

it('marks the current line in the viewer', function () {
    $browser = viewer();

    $frame = function () use ($browser) {
        $method = new ReflectionMethod($browser, 'renderTheme');
        $method->setAccessible(true);

        return preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser));
    };

    $lineOf = function (string $frame) {
        foreach (explode("\n", $frame) as $index => $line) {
            if (str_contains($line, '▸')) {
                return $index;
            }
        }

        return null;
    };

    $first = $lineOf($frame());

    expect($first)->not->toBeNull();

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($lineOf($frame()))->toBe($first + 2);
});

it('does not mark lines while editing, where the text cursor shows instead', function () {
    $browser = viewer();
    $browser->emit('key', "\e");
    $browser->emit('key', 'e');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $raw = $method->invoke($browser);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $raw))->not->toContain('▸')
        ->and($raw)->toContain("\e[7m");
});

it('jumps to a line number typed before G', function () {
    $browser = viewer();

    $browser->emit('key', '3');
    $browser->emit('key', 'G');

    expect($browser->cellEditor->cursorLine())->toBe(2);

    $browser->emit('key', '1');
    $browser->emit('key', 'G');

    expect($browser->cellEditor->cursorLine())->toBe(0);
});

it('still jumps to the bottom with a bare G', function () {
    $browser = viewer();

    $browser->emit('key', 'G');

    expect($browser->cellEditor->cursorLine())->toBe(count($browser->cellEditor->lines()) - 1);
});

it('repeats movement with a count', function () {
    $browser = viewer();

    $browser->emit('key', '3');
    $browser->emit('key', 'j');

    expect($browser->cellEditor->cursorLine())->toBe(3);

    $browser->emit('key', '2');
    $browser->emit('key', 'k');

    expect($browser->cellEditor->cursorLine())->toBe(1);
});

it('clears the count after using it', function () {
    $browser = viewer();

    $browser->emit('key', '3');
    $browser->emit('key', 'j');
    $browser->emit('key', 'G');

    expect($browser->cellEditor->cursorLine())->toBe(count($browser->cellEditor->lines()) - 1);
});

it('clicks a line in the viewer', function () {
    config(['dotsql.ui.mouse_row_offset' => 0]);

    $browser = viewer();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);

    $island = $browser->valueIsland;

    expect($island)->not->toBeNull();

    $row = $island->y + 1 + 2;

    $browser->emit('key', "\e[<0;6;{$row}M");

    expect($browser->cellEditor->cursorLine())->toBe($island->lineAt(2));
});

it('drags to select lines in the viewer', function () {
    config(['dotsql.ui.mouse_row_offset' => 0]);

    $browser = viewer();

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);
    $method->invoke($browser);

    $island = $browser->valueIsland;
    $row = $island->y + 1;

    $browser->emit('key', "\e[<0;6;{$row}M");
    $method->invoke($browser);

    $browser->emit('key', "\e[<32;6;".($row + 2).'M');
    $method->invoke($browser);

    [$from, $to] = $browser->selectedLines();

    expect($to - $from)->toBe(2)
        ->and($browser->visualAnchor)->not->toBeNull();
});
