<?php

use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\Styler;
use App\Tui\QueryEditor;

function wrapStyle(): Styler
{
    $same = fn (string $text) => $text;

    return new Styler($same, $same, $same, $same, fn (string $text, int $width) => mb_substr($text, 0, $width), fn (string $name, string $text) => $name === 'cursor' ? "[{$text}]" : $text);
}

function wrapped(string $sql, int $width, int $height = 10, ?int $cursor = null): array
{
    $editor = new QueryEditor;
    $editor->set($sql);

    if ($cursor !== null) {
        $editor->toLineColumn(0, $cursor);
    }

    $island = new EditorIsland($editor, true, null, wrapStyle());

    return [$island, $island->content($width, $height)];
}

it('wraps a long line at a space so every row fits the pane', function () {
    [, $rows] = wrapped('select id, name, email from customers where name like "%ada%"', 20, 10, 0);

    expect($rows)->toHaveCount(4)
        ->and($rows[0])->toBe('[s]elect id, name, ')
        ->and($rows[1])->toBe('email from ')
        ->and(max(array_map(fn (string $row) => mb_strlen(str_replace(['[', ']'], '', $row)), $rows)))->toBeLessThanOrEqual(20);
});

it('breaks a word wider than the pane where it has to', function () {
    [, $rows] = wrapped(str_repeat('x', 25), 10, 10, 0);

    expect(array_map(fn (string $row) => str_replace(['[', ']'], '', $row), $rows))->toBe(['xxxxxxxxxx', 'xxxxxxxxxx', 'xxxxx']);
});

it('keeps the caret in view at the end of a long line', function () {
    [, $rows] = wrapped('select * from customers', 10);

    expect(end($rows))->toEndWith('[ ]');
});

it('scrolls to the row the caret is on', function () {
    [$island, $rows] = wrapped(str_repeat('word ', 30), 10, 3);

    expect($rows)->toHaveCount(3)
        ->and(end($rows))->toContain('[ ]')
        ->and($island->positionAt(0, 0)[1])->toBeGreaterThan(0);
});

it('maps a click on a wrapped row back to the right place in the line', function () {
    [$island] = wrapped('select id, name, email from customers', 20, 10, 0);

    expect($island->positionAt(1, 2))->toBe([0, 19])
        ->and($island->positionAt(0, 3))->toBe([0, 3]);
});
