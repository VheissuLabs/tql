<?php

use App\Models\QueryExecution;
use App\Tui\History;
use Laravel\Prompts\Key;

function ran(object $browser, string $statement, string $source = 'editor', bool $succeeded = true, ?int $rows = 1): QueryExecution
{
    return QueryExecution::create([
        'connection_id' => $browser->connection->id,
        'statement' => $statement,
        'source' => $source,
        'succeeded' => $succeeded,
        'row_count' => $rows,
        'duration_ms' => 1,
    ]);
}

it('records a statement run from the SQL editor as editor', function () {
    $browser = sqlPane();

    typeSql($browser, 'select name from fruit');
    $browser->emit('key', "\x12");

    expect(QueryExecution::where('connection_id', $browser->connection->id)->where('source', 'editor')->pluck('statement')->all())
        ->toBe(['select name from fruit']);
});

it('lists what you and the agent ran, newest first, once each, and leaves out what tql ran itself', function () {
    $browser = sqlPane();

    ran($browser, 'select 1');
    ran($browser, 'select * from "fruit" limit 50 offset 0', 'tui');
    ran($browser, 'select 2', 'mcp');
    ran($browser, 'select 1');

    expect(array_column(History::of($browser->connection), 'statement'))->toBe(['select 1', 'select 2']);
});

it('keeps each connection to its own history', function () {
    $browser = sqlPane();
    $other = sqlPane();

    ran($browser, 'select 1');
    ran($other, 'select 2');

    expect(array_column(History::of($browser->connection), 'statement'))->toBe(['select 1']);
});

it('opens with H, filters as you type, and puts the statement in the editor on enter without running it', function () {
    $browser = sqlPane();
    $browser->emit('key', Key::ESCAPE);

    ran($browser, "select name\n  from fruit");
    ran($browser, 'select qty from fruit', 'mcp', rows: 3);

    $browser->emit('key', 'H');

    $frame = paintSql($browser);

    expect($browser->palette)->not->toBeNull()
        ->and($frame)->toContain('HISTORY')
        ->and($frame)->toContain('select name from fruit')
        ->and($frame)->toContain('mcp');

    typeSql($browser, 'name');
    $browser->emit('key', Key::ENTER);

    expect($browser->palette)->toBeNull()
        ->and($browser->mode)->toBe('query')
        ->and($browser->editor->buffer())->toBe("select name\n  from fruit")
        ->and($browser->resultsFromQuery)->toBeFalse();
});

it('opens with alt+h from inside the SQL editor', function (string $style) {
    $browser = sqlPane($style);

    ran($browser, 'select qty from fruit');

    $browser->emit('key', "\eh");

    expect($browser->palette)->not->toBeNull();

    $browser->emit('key', Key::ENTER);

    expect($browser->editor->buffer())->toBe('select qty from fruit');
})->with(['simple', 'vim']);

it('opens from the command line', function () {
    $browser = sqlPane();
    $browser->emit('key', Key::ESCAPE);

    ran($browser, 'select 1');

    typeSql($browser, ':history');
    $browser->emit('key', Key::ENTER);

    expect($browser->palette)->not->toBeNull()
        ->and(paintSql($browser))->toContain('HISTORY');
});

it('marks a statement that failed', function () {
    $browser = sqlPane();
    $browser->emit('key', Key::ESCAPE);

    ran($browser, 'select nope from fruit', succeeded: false, rows: null);

    $browser->emit('key', 'H');

    expect(paintSql($browser))->toContain('failed');
});

it('says so when there is no history yet', function () {
    $browser = sqlPane();
    $browser->emit('key', Key::ESCAPE);

    $browser->emit('key', 'H');

    expect($browser->palette)->toBeNull()
        ->and($browser->status)->toContain('no history');
});
