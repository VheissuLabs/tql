<?php

use App\Models\QueryExecution;
use Laravel\Prompts\Key;

function joinedKey(object $browser, string $key, ?string $following): string
{
    return (new ReflectionMethod($browser, 'joinSplitEscape'))->invoke($browser, $key, fn () => $following);
}

it('reads an escape followed at once by a key as one alt key', function () {
    $browser = sqlPane('vim');

    expect(joinedKey($browser, Key::ESCAPE, 'h'))->toBe("\eh");
});

it('keeps an escape on its own when nothing follows it', function () {
    $browser = sqlPane('vim');

    expect(joinedKey($browser, Key::ESCAPE, null))->toBe(Key::ESCAPE);
});

it('leaves every other key alone', function () {
    $browser = sqlPane('vim');

    expect(joinedKey($browser, 'h', 'j'))->toBe('h');
});

it('opens the history from vim insert mode with alt+h and keeps you able to type', function () {
    $browser = sqlPane('vim');

    QueryExecution::create(['connection_id' => $browser->connection->id, 'statement' => 'select 1', 'source' => 'editor', 'succeeded' => true, 'duration_ms' => 1]);

    $browser->emit('key', 'i');
    $browser->emit('key', joinedKey($browser, Key::ESCAPE, 'h'));

    expect($browser->palette)->not->toBeNull();

    $browser->emit('key', Key::ESCAPE);
    typeSql($browser, 'abc');

    expect($browser->sqlMode)->toBe('insert')
        ->and($browser->editor->buffer())->toBe('abc');
});
