<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
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
    return $browser->cellEditor?->buffer() ?? '';
}

it('shows the whole row as an object on i', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('edit')
        ->and($browser->inspectingRow)->toBeTrue()
        ->and($browser->editable)->toBeFalse();

    $object = json_decode(inspected($browser), true);

    expect($object)->toHaveKeys(['id', 'name', 'payload', 'created_at'])
        ->and($object['name'])->toBe('user.signed_up')
        ->and($object['created_at'])->toBe('2026-09-22T15:11:32+00:00');
});

it('unwraps a json column rather than nesting a string of json', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    $object = json_decode(inspected($browser), true);

    expect($object['payload'])->toBeArray()
        ->and($object['payload']['user']['email'])->toBe('karl@example.com');
});

it('titles the modal with the row key', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');

    expect($browser->cellColumn())->toBe('row id 1');
});

it('still shows one value on shift+i', function () {
    $browser = inspectable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'I');

    expect($browser->inspectingRow)->toBeFalse()
        ->and(inspected($browser))->toBe('user.signed_up')
        ->and($browser->cellColumn())->toBe('name');
});

it('scrolls and yanks like the value viewer', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', 'j');
    $browser->emit('key', 'V');
    $browser->emit('key', 'j');

    expect($browser->visualAnchor)->not->toBeNull()
        ->and($browser->selectedLines())->toBe([1, 2]);
});

it('opens the editor on the cell with e', function () {
    $browser = inspectable();

    $browser->emit('key', 'l');
    $browser->emit('key', 'i');

    expect($browser->inspectingRow)->toBeTrue();

    $browser->emit('key', 'e');

    expect($browser->inspectingRow)->toBeFalse()
        ->and($browser->editable)->toBeTrue()
        ->and(inspected($browser))->toBe('user.signed_up');
});

it('closes on escape without saying it cancelled an edit', function () {
    $browser = inspectable();

    $browser->emit('key', 'i');
    $browser->emit('key', "\e");

    expect($browser->mode)->toBe('browse')
        ->and($browser->inspectingRow)->toBeFalse()
        ->and($browser->status)->toBeNull();
});

it('says so when there is no row to inspect', function () {
    $browser = inspectable();

    $browser->raw = [];
    $browser->rows = [];

    $browser->emit('key', 'i');

    expect($browser->mode)->toBe('browse')
        ->and($browser->status)->toContain('no rows');
});
