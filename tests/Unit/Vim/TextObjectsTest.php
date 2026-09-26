<?php

use App\Tui\Vim\Range;
use App\Tui\Vim\Text;
use App\Tui\Vim\TextObjects;

function textAt(string $marked): array
{
    $at = mb_strpos($marked, '|');

    return [Text::of(str_replace('|', '', $marked)), $at];
}

function covered(Text $text, ?Range $range): ?string
{
    if ($range === null) {
        return null;
    }

    return implode('', array_slice($text->chars, $range->start, $range->end - $range->start));
}

it('selects a word inside or around', function (string $from, bool $around, bool $big, string $expected) {
    [$text, $at] = textAt($from);

    expect(covered($text, TextObjects::word($text, $at, $around, $big)))->toBe($expected);
})->with([
    ['select na|me from', false, false, 'name'],
    ['select na|me from', true, false, 'name '],
    ['select name fr|om', true, false, ' from'],
    ['fruit.na|me x', false, false, 'name'],
    ['fruit.na|me x', false, true, 'fruit.name'],
    ['select |  name', false, false, '   '],
    ['select |  name', true, false, '   name'],
]);

it('selects inside or around brackets, across lines', function (string $from, string $open, bool $around, ?string $expected) {
    [$text, $at] = textAt($from);

    expect(covered($text, TextObjects::bracket($text, $at, $open, $around)))->toBe($expected);
})->with([
    ['count(i|d)', '(', false, 'id'],
    ['count(i|d)', '(', true, '(id)'],
    ['coalesce(a, max(|b))', '(', false, 'b'],
    ['coalesce(a, |max(b))', '(', false, 'a, max(b)'],
    ["in (\n  sel|ect 1\n)", '(', false, "\n  select 1\n"],
    ['count|(id)', '(', false, 'id'],
    ['count(id|)', '(', false, 'id'],
    ['tags[|0]', '[', false, '0'],
    ['{"a": |1}', '{', true, '{"a": 1}'],
    ['sel|ect 1', '(', false, null],
]);

it('selects inside or around quotes on the line', function (string $from, string $quote, bool $around, ?string $expected) {
    [$text, $at] = textAt($from);

    expect(covered($text, TextObjects::quote($text, $at, $quote, $around)))->toBe($expected);
})->with([
    ["where name = 'ch|erry'", "'", false, 'cherry'],
    ["where name = 'ch|erry' and", "'", true, "'cherry' "],
    ["wh|ere name = 'cherry'", "'", false, 'cherry'],
    ["where a = 'x' and b = 'y|y'", "'", false, 'yy'],
    ["where a = |'x'", "'", false, 'x'],
    ['select "na|me"', '"', false, 'name'],
    ["where name = 'ch|erry", "'", false, null],
]);

it('selects a paragraph line-wise', function (string $from, bool $around, string $expected) {
    [$text, $at] = textAt($from);
    $range = TextObjects::paragraph($text, $at, $around);

    expect($range->linewise)->toBeTrue()
        ->and(covered($text, $range))->toBe($expected);
})->with([
    ["select\nfr|om\n\nwhere", false, "select\nfrom"],
    ["select\nfr|om\n\n\nwhere", true, "select\nfrom\n\n"],
    ["select\n\nwh|ere\nand", true, "\nwhere\nand"],
]);
