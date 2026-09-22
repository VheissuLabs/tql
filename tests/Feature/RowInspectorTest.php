<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowDocument;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function inspectable(): Browser
{
    $path = sys_get_temp_dir().'/tql-inspect-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table events (id integer primary key, name text, payload text, created_at text)');
    $pdo->prepare('insert into events (name, payload, created_at) values (?, ?, ?)')->execute([
        'user.signed_up',
        '{"user":{"id":42,"email":"karl@example.com"}}',
        '2026-09-22T15:11:32+00:00',
    ]);

    $connection = Connection::create([
        'name' => 'inspect'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $render->invoke($browser);

    return $browser;
}

function inspected(Browser $browser): string
{
    return $browser->document?->text() ?? '';
}

function inspectorLines(Browser $browser): array
{
    return array_column($browser->document->lines(), 'text');
}

it('shows the record and its types on i', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('inspect')
        ->and($browser->document)->not->toBeNull();

    $text = inspected($browser);

    expect($text)->toContain('RECORD  (4)')
        ->and($text)->toContain('name')
        ->and($text)->toContain('user.signed_up')
        ->and($text)->toContain('created_at')
        ->and($text)->toContain('text')
        ->and($text)->toContain('integer');
});

it('folds a section with enter', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect(inspected($browser))->toContain('user.signed_up');

    // The cursor starts on the record heading.
    $browser->emit('key', "\n");

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeTrue()
        ->and(inspected($browser))->not->toContain('user.signed_up');

    $browser->emit('key', "\n");

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeFalse()
        ->and(inspected($browser))->toContain('user.signed_up');
});

it('folds with space as well as enter', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', ' ');

    expect($browser->document->isFolded(RowDocument::RECORD))->toBeTrue();
});

it('does nothing when the line is not foldable', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'j');

    $before = inspected($browser);

    $browser->emit('key', "\n");

    expect(inspected($browser))->toBe($before);
});

it('moves with j and k and jumps with g and G', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->documentLine)->toBe(0);

    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    expect($browser->documentLine)->toBe(2);

    $browser->emit('key', 'g');

    expect($browser->documentLine)->toBe(0);

    $browser->emit('key', 'G');

    expect($browser->documentLine)->toBe(count($browser->document->lines()) - 1);
});

it('keeps the cursor in range when a fold shortens the document', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'G');

    $bottom = $browser->documentLine;

    $browser->emit('key', 'g');
    $browser->emit('key', "\n");

    expect($browser->documentLine)->toBeLessThanOrEqual(count($browser->document->lines()) - 1)
        ->and($bottom)->toBeGreaterThan(0);
});

it('draws each section as its own box', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // This table has no foreign keys, so there is nothing to relate.
    expect($frame)->toContain('┌─ RECORD  (4)')
        ->and($frame)->not->toContain('RELATED');
});

it('collapses a box to its title bar', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $open = substr_count(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)), "\n");

    $browser->emit('key', "\n");

    $closed = substr_count(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)), "\n");

    // Folding takes rows off the screen, rather than emptying a box that
    // stays the same size.
    expect($closed)->toBe($open);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // The box is down to its title bar: no bottom border follows it.
    expect($frame)->toContain('┌─ RECORD  (4)')
        ->and($browser->document->section(RowDocument::RECORD))->toBe([]);
});

it('floats over the grid rather than taking the screen', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $frame = preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));

    // The panes are still there behind it.
    expect($frame)->toContain('TABLES')
        ->and($frame)->toContain('┌─ RECORD')
        ->and($frame)->toContain('events');
});

it('keeps the inspector inside the terminal at any size', function () {
    foreach ([[100, 24], [140, 30], [90, 20], [200, 50]] as [$cols, $rows]) {
        putenv("COLUMNS={$cols}");
        putenv("LINES={$rows}");

        $browser = inspectable();
        $browser->emit('key', 'i');

        $render = new ReflectionMethod($browser, 'renderTheme');
        $render->setAccessible(true);

        $lines = explode("\n", preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)));

        expect(max(array_map('mb_strlen', $lines)))->toBeLessThanOrEqual($cols)
            ->and(count($lines))->toBeLessThanOrEqual($rows);
    }

    putenv('COLUMNS');
    putenv('LINES');
});

it('still shows one value on shift+i', function () {
    $browser = inspectable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'I');

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellEditor?->buffer())->toBe('user.signed_up')
        ->and($browser->cellColumn())->toBe('name');
});

it('selects and yanks lines', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'j');
    $browser->emit('key', 'V');
    $browser->emit('key', 'j');

    expect($browser->documentSelection())->toBe([1, 2]);

    $browser->emit('key', 'y');

    expect($browser->status)->toContain('yanked 2 lines')
        ->and($browser->documentAnchor)->toBeNull();
});

it('clears a selection with escape before closing', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'V');
    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('inspect')
        ->and($browser->documentAnchor)->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse');
});

it('edits the column the cursor is on with e', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    // record heading, then id, then name.
    $browser->emit('key', 'j');
    $browser->emit('key', 'j');

    $browser->emit('key', 'e');

    expect($browser->mode)->toBe('edit')
        ->and($browser->cellColumn())->toBe('name')
        ->and($browser->cellEditor?->buffer())->toBe('user.signed_up');
});

it('says so when there is no row to inspect', function () {
    $browser = inspectable();

    $browser->raw = [];
    $browser->rows = [];

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('browse')
        ->and($browser->status)->toContain('no rows');
});
