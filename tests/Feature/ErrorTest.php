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

function refusing(): Browser
{
    $path = sys_get_temp_dir().'/tql-err-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table people (id integer primary key, name text not null, email text unique)');
    $pdo->exec("insert into people (name, email) values ('Ada', 'ada@example.com')");

    $connection = Connection::create([
        'name' => 'err'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

function shown(Browser $browser): string
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));
}

it('opens a modal when the database refuses a write', function () {
    $browser = refusing();

    $browser->emit('key', 'N');
    $browser->emit('key', "\x13");
    $browser->columnIndex = 1;
    $browser->emit('key', 'e');

    foreach (mb_str_split('Grace') as $character) {
        $browser->emit('key', $character);
    }

    $browser->emit('key', "\n");

    $browser->columnIndex = 2;
    $browser->emit('key', 'e');

    foreach (mb_str_split('ada@example.com') as $character) {
        $browser->emit('key', $character);
    }

    $browser->emit('key', "\n");

    $browser->emit('key', ':');
    $browser->emit('key', 'w');
    $browser->emit('key', "\n");

    $text = shown($browser);

    expect($browser->problem)->not->toBeNull()
        ->and($text)->toContain('COULD NOT ADD THE ROW')
        ->and($text)->toContain('UNIQUE constraint')
        // The statement it refused, on its own, and what to do next.
        ->and($text)->toContain('insert into "people"')
        ->and($text)->toContain('u to drop it')
        // And the row is still there to fix.
        ->and($browser->pendingInserts)->toHaveCount(1);
});

it('takes the plumbing out of the message', function () {
    $browser = refusing();

    $browser->emit('key', 's');
    $browser->editor->set('select * from nowhere');
    $browser->emit('key', "\n");

    expect($browser->problem)->toContain('no such table')
        // Not the connection name tql made up, nor the file it opened.
        ->and($browser->problem)->not->toContain('Connection: ')
        ->and($browser->problem)->not->toContain('Database: ')
        ->and(shown($browser))->toContain('THE DATABASE SAID NO');
});

it('closes on a key and leaves the screen as it was', function () {
    $browser = refusing();

    $browser->emit('key', 's');
    $browser->editor->set('select * from nowhere');
    $browser->emit('key', "\n");

    expect($browser->problem)->not->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->problem)->toBeNull()
        ->and(shown($browser))->not->toContain('THE DATABASE SAID NO');
});

it('copies the error with y', function () {
    $browser = refusing();

    $browser->emit('key', 's');
    $browser->editor->set('select * from nowhere');
    $browser->emit('key', "\n");

    $browser->emit('key', 'y');

    expect($browser->problem)->not->toBeNull()
        ->and($browser->status)->toContain('copied');
});

it('takes every key while it is up, so nothing happens behind it', function () {
    $browser = refusing();

    $browser->emit('key', 's');
    $browser->editor->set('select * from nowhere');
    $browser->emit('key', "\n");

    // d would mark a row for deletion if the error were not in front.
    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toBe([])
        ->and($browser->problem)->toBeNull();
});

it('scrolls a long message', function () {
    $browser = refusing();

    $browser->fail(implode("\n", array_fill(0, 40, 'a line of the message')));

    $browser->emit('key', 'j');

    expect($browser->problemOffset)->toBe(1);

    $browser->emit('key', 'k');
    $browser->emit('key', 'k');

    expect($browser->problemOffset)->toBe(0);
});
