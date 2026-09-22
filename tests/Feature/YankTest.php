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
