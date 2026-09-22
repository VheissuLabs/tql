<?php

use App\Commands\BrowseCommand;
use App\Models\Connection;
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

it('asks to edit the highlighted connection', function () {
    $picker = picker();

    $picker->emit('key', 'e');

    $first = Connection::orderBy('name')->first();

    expect($picker->value())->toBe('edit:'.$first->id);
});

it('edits the one the cursor moved to', function () {
    $picker = picker();

    $picker->emit('key', 'j');
    $picker->emit('key', 'e');

    $second = Connection::orderBy('name')->skip(1)->first();

    expect($picker->value())->toBe('edit:'.$second->id);
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

function editParts(Connection $connection): array
{
    $command = app(BrowseCommand::class);

    $options = new ReflectionMethod($command, 'editOptions');
    $options->setAccessible(true);

    return $options->invoke($command, $connection);
}

it('offers a way out of editing', function () {
    $connection = Connection::create(['name' => 'edits', 'driver' => 'sqlite', 'database' => '/tmp/a.sqlite']);

    expect(editParts($connection))->toHaveKey('cancel')
        ->and(editParts($connection))->toHaveKey('save');
});

it('offers only the fields that driver has', function () {
    $sqlite = Connection::create(['name' => 'lite', 'driver' => 'sqlite', 'database' => '/tmp/a.sqlite']);

    expect(array_keys(editParts($sqlite)))->toBe(['name', 'database', 'save', 'cancel']);

    $mysql = Connection::create([
        'name' => 'my', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
        'database' => 'shop', 'username' => 'alice', 'password' => 'pw',
    ]);

    expect(array_keys(editParts($mysql)))
        ->toBe(['name', 'host', 'port', 'database', 'username', 'password', 'save', 'cancel']);
});

it('shows the current value beside each field but never the password', function () {
    $connection = Connection::create([
        'name' => 'shown', 'driver' => 'mysql', 'host' => 'db.example.com', 'port' => 3306,
        'database' => 'shop', 'username' => 'alice', 'password' => 'hunter2',
    ]);

    $options = editParts($connection);

    expect($options['host'])->toContain('db.example.com')
        ->and($options['username'])->toContain('alice')
        ->and($options['password'])->toContain('••••')
        ->and($options['password'])->not->toContain('hunter2');
});

it('says when a password is not set', function () {
    $connection = Connection::create(['name' => 'nopw', 'driver' => 'mysql', 'host' => 'h', 'port' => 3306]);

    expect(editParts($connection)['password'])->toContain('not set');
});
