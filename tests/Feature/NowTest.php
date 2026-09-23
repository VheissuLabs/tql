<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Support\Now;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function timestamped(): Browser
{
    $path = sys_get_temp_dir().'/tql-now-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table posts (id integer primary key, title text, published_on date, starts_at time, created_at datetime)');
    $pdo->exec("insert into posts (title) values ('hello')");

    $connection = Connection::create([
        'name' => 'now'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    return new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
}

function fill(Browser $browser, string $column, string $text): void
{
    $browser->columnIndex = (int) array_search($column, $browser->headers, true);
    $browser->emit('key', 'e');

    foreach (mb_str_split($text) as $character) {
        $browser->emit('key', $character);
    }

    $browser->emit('key', "\n");
}

it('writes the time the way the column takes it', function () {
    expect(Now::for('date'))->toBe(date('Y-m-d'))
        ->and(Now::for('time'))->toBe(date('H:i:s'))
        ->and(Now::for('year'))->toBe(date('Y'))
        ->and(Now::for('datetime'))->toBe(date('Y-m-d H:i:s'))
        ->and(Now::for('timestamp'))->toBe(date('Y-m-d H:i:s'))
        // An unknown type still gets something a database will take.
        ->and(Now::for('text'))->toBe(date('Y-m-d H:i:s'));
});

it('knows which columns hold a moment', function () {
    expect(Now::suits('datetime'))->toBeTrue()
        ->and(Now::suits('timestamp without time zone'))->toBeTrue()
        ->and(Now::suits('DATE'))->toBeTrue()
        ->and(Now::suits('year'))->toBeTrue()
        ->and(Now::suits('varchar'))->toBeFalse()
        ->and(Now::suits(null))->toBeFalse();
});

it('turns now() into the time, per column', function () {
    $browser = timestamped();

    $browser->emit('key', 'N');
    $browser->emit('key', "\x13");

    fill($browser, 'published_on', 'now()');
    fill($browser, 'starts_at', 'NOW()');
    fill($browser, 'created_at', 'now()');

    expect($browser->pendingInserts[0]['published_on'])->toBe(date('Y-m-d'))
        // However it was typed.
        ->and($browser->pendingInserts[0]['starts_at'])->toBe(date('H:i:s'))
        ->and($browser->pendingInserts[0]['created_at'])->toBe(date('Y-m-d H:i:s'));
});

it('types the time with ctrl+t', function () {
    $browser = timestamped();

    $browser->emit('key', 'N');
    $browser->emit('key', "\x13");

    $browser->columnIndex = (int) array_search('created_at', $browser->headers, true);
    $browser->emit('key', 'e');

    expect($browser->editingTime())->toBeTrue();

    $browser->emit('key', "\x14");

    // It goes into the editor first, so you can see it before keeping it.
    expect($browser->cellEditor->buffer())->toBe(date('Y-m-d H:i:s'));

    $browser->emit('key', "\n");

    expect($browser->pendingInserts[0]['created_at'])->toBe(date('Y-m-d H:i:s'));
});

it('leaves now() alone in a column that is not a time', function () {
    $browser = timestamped();

    $browser->emit('key', 'N');
    $browser->emit('key', "\x13");

    fill($browser, 'title', 'now()');

    // Still the time: an explicit now() is an instruction wherever it is
    // typed, and the column decides only the format.
    expect($browser->pendingInserts[0]['title'])->toBe(date('Y-m-d H:i:s'))
        ->and($browser->editingTime())->toBeFalse();
});

it('says ctrl+t is there while editing a time', function () {
    $browser = timestamped();

    $browser->columnIndex = (int) array_search('created_at', $browser->headers, true);
    $browser->emit('key', 'e');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))->toContain('ctrl+t now');
});

it('writes UTC unless the config says otherwise', function () {
    config(['tql.ui.time_zone' => 'UTC']);

    expect(Now::for('datetime'))->toBe(gmdate('Y-m-d H:i:s'))
        ->and(Now::label())->toBe('UTC');

    config(['tql.ui.time_zone' => 'Asia/Kolkata']);

    expect(Now::for('datetime'))
        ->toBe((new DateTimeImmutable('now', new DateTimeZone('Asia/Kolkata')))->format('Y-m-d H:i:s'))
        ->and(Now::for('datetime'))->not->toBe(gmdate('Y-m-d H:i:s'));

    // A zone nobody has heard of is not a reason to write the wrong time.
    config(['tql.ui.time_zone' => 'Nowhere/Special']);

    expect(Now::for('datetime'))->toBe(gmdate('Y-m-d H:i:s'));

    config(['tql.ui.time_zone' => 'UTC']);
});

it('says which zone it is about to write', function () {
    config(['tql.ui.time_zone' => 'America/Toronto']);

    $browser = timestamped();

    $browser->columnIndex = (int) array_search('created_at', $browser->headers, true);
    $browser->emit('key', 'e');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))
        ->toContain('ctrl+t now E');

    config(['tql.ui.time_zone' => 'UTC']);
});
