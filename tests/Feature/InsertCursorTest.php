<?php

use Laravel\Prompts\Key;

function rawFrame(object $browser): string
{
    return (new ReflectionMethod($browser, 'renderTheme'))->invoke($browser);
}

function editorLine(string $frame, string $typed): string
{
    $line = collect(explode("\n", $frame))->first(fn (string $line) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), $typed));

    $keepingInverse = preg_replace('/\e\[(?!7m)[0-9;]*m/', '', $line);

    return substr($keepingInverse, (int) strpos($keepingInverse, $typed));
}

function terminalCursorFor(object $browser, string $frame): string
{
    return (new ReflectionMethod($browser, 'placeTerminalCursor'))->invoke($browser, $frame);
}

it('puts the terminal cursor at the insert point as a line in vim insert mode', function () {
    $browser = sqlPane('vim');

    $browser->emit('key', 'i');
    typeSql($browser, 'select');

    $frame = rawFrame($browser);
    $lines = explode("\n", $frame);
    $row = collect($lines)->search(fn (string $line) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 'select')) + 1;
    $column = $browser->editorIsland->contentColumn(6);

    expect(terminalCursorFor($browser, $frame))
        ->toBe("\e[".(count($lines) - $row)."A\e[{$column}G\e[6 q\e[?25h")
        ->and(editorLine($frame, 'select'))->not->toContain("\e[7m");
});

it('keeps the painted block and the hidden terminal cursor in vim normal mode', function () {
    $browser = sqlPane('vim');

    $browser->emit('key', 'i');
    typeSql($browser, 'select');
    $browser->emit('key', Key::ESCAPE);

    $frame = rawFrame($browser);

    expect(terminalCursorFor($browser, $frame))->toBe('')
        ->and(editorLine($frame, 'select'))->toContain("\e[7m");
});

it('keeps the block in the simple editor', function () {
    $browser = sqlPane();

    typeSql($browser, 'select');

    $frame = rawFrame($browser);

    expect(terminalCursorFor($browser, $frame))->toBe('')
        ->and(editorLine($frame, 'select'))->toContain("\e[7m");
});

it('takes the terminal cursor back down and restores its shape once insert mode ends', function () {
    $browser = sqlPane('vim');

    $browser->emit('key', 'i');
    typeSql($browser, 'select');

    $frame = rawFrame($browser);
    $lifted = count(explode("\n", $frame)) - 1 - collect(explode("\n", $frame))->search(fn (string $line) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 'select'));

    terminalCursorFor($browser, $frame);

    $browser->emit('key', Key::ESCAPE);

    $lower = new ReflectionMethod($browser, 'lowerTerminalCursor');

    expect($lower->invoke($browser))->toBe("\e[?25l\e[{$lifted}B")
        ->and(terminalCursorFor($browser, rawFrame($browser)))->toBe("\e[0 q")
        ->and($lower->invoke($browser))->toBe('');
});
