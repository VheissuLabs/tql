<?php

use App\Tui\Vim\Motions;
use App\Tui\Vim\Text;

function cursorIn(string $marked): array
{
    $at = mb_strpos($marked, '|');

    return [Text::of(str_replace('|', '', $marked)), $at];
}

function marked(Text $text, ?int $at): ?string
{
    if ($at === null) {
        return null;
    }

    return implode('', array_slice($text->chars, 0, $at)).'|'.implode('', array_slice($text->chars, $at));
}

it('moves left and right without leaving the line', function (string $from, string $key, int $count, string $to) {
    [$text, $at] = cursorIn($from);

    $moved = $key === 'h'
        ? Motions::left($text, $at, $count)
        : Motions::right($text, $at, $count);

    expect(marked($text, $moved))->toBe($to);
})->with([
    ["sel|ect\nfrom", 'h', 1, "se|lect\nfrom"],
    ["sel|ect\nfrom", 'h', 9, "|select\nfrom"],
    ["sel|ect\nfrom", 'l', 2, "selec|t\nfrom"],
    ["sel|ect\nfrom", 'l', 9, "select|\nfrom"],
]);

it('moves down and up to the column it can', function (string $from, int $by, int $column, ?string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::vertical($text, $at, $by, $column)))->toBe($to);
})->with([
    ["sel|ect *\nfrom fruit", 1, 3, "select *\nfro|m fruit"],
    ["select *\nfrom fr|uit", -1, 7, "select |*\nfrom fruit"],
    ["select *\nab\nfrom fruit|", -1, 10, "select *\na|b\nfrom fruit"],
    ["select *\n\nfrom fruit|", -1, 10, "select *\n|\nfrom fruit"],
    ["a|\nb\nc", 5, 0, "a\nb\n|c"],
    ["a\nb\n|c", 1, 0, null],
    ["|a\nb", -1, 0, null],
]);

it('finds the line start, first non-blank and last character', function () {
    [$text, $at] = cursorIn("select\n    from fr|uit\nwhere");

    expect(marked($text, Motions::lineStart($text, $at)))->toBe("select\n|    from fruit\nwhere")
        ->and(marked($text, Motions::firstNonBlank($text, $at)))->toBe("select\n    |from fruit\nwhere")
        ->and(marked($text, Motions::lastCharacter($text, $at, 1)))->toBe("select\n    from frui|t\nwhere")
        ->and(marked($text, Motions::lastCharacter($text, $at, 2)))->toBe("select\n    from fruit\nwher|e");
});

it('goes to a numbered line at its first non-blank', function () {
    [$text] = cursorIn("|select\n  from fruit\nwhere");

    expect(marked($text, Motions::line($text, 1)))->toBe("select\n  |from fruit\nwhere")
        ->and(marked($text, Motions::line($text, 99)))->toBe("select\n  from fruit\n|where");
});

it('moves forward by word', function (string $from, int $count, bool $big, string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::wordForward($text, $at, $count, $big)))->toBe($to);
})->with([
    ['|select name from fruit', 1, false, 'select |name from fruit'],
    ['|select name from fruit', 3, false, 'select name from |fruit'],
    ['|fruit.name, qty', 1, false, 'fruit|.name, qty'],
    ['fruit|.name, qty', 1, false, 'fruit.|name, qty'],
    ['|fruit.name, qty', 1, true, 'fruit.name, |qty'],
    ["sel|ect\n  from", 1, false, "select\n  |from"],
    ["sel|ect\n\nfrom", 1, false, "select\n|\nfrom"],
    ['select fr|uit', 1, false, 'select fruit|'],
]);

it('moves back by word', function (string $from, int $count, bool $big, string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::wordBackward($text, $at, $count, $big)))->toBe($to);
})->with([
    ['select name from fr|uit', 1, false, 'select name from |fruit'],
    ['select name from |fruit', 2, false, 'select |name from fruit'],
    ['fruit.name|, qty', 1, false, 'fruit.|name, qty'],
    ['fruit.name, q|ty', 2, true, '|fruit.name, qty'],
    ["select\n  |from", 1, false, "|select\n  from"],
    ["select\n\n|from", 1, false, "select\n|\nfrom"],
    ['|select', 1, false, '|select'],
]);

it('moves to the end of a word', function (string $from, int $count, bool $big, string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::wordEnd($text, $at, $count, $big)))->toBe($to);
})->with([
    ['|select name', 1, false, 'selec|t name'],
    ['selec|t name', 1, false, 'select nam|e'],
    ['|fruit.name', 1, false, 'frui|t.name'],
    ['|fruit.name x', 1, true, 'fruit.nam|e x'],
    ['|select name from', 3, false, 'select name fro|m'],
]);

it('finds a character in the line', function (string $from, string $char, int $count, bool $forward, bool $till, ?string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::findInLine($text, $at, $char, $count, $forward, $till)))->toBe($to);
})->with([
    ['|select a, b, c', ',', 1, true, false, 'select a|, b, c'],
    ['|select a, b, c', ',', 2, true, false, 'select a, b|, c'],
    ['|select a, b, c', ',', 1, true, true, 'select |a, b, c'],
    ['select a, b, |c', ',', 1, false, false, 'select a, b|, c'],
    ['select a, b, |c', ',', 1, false, true, 'select a, b,| c'],
    ["|select\na, b", ',', 1, true, false, null],
    ['|select', 'z', 1, true, false, null],
]);

it('jumps to the matching bracket', function (string $from, ?string $to) {
    [$text, $at] = cursorIn($from);

    expect(marked($text, Motions::matchingBracket($text, $at)))->toBe($to);
})->with([
    ['count|(id)', 'count(id|)'],
    ['count(id|)', 'count|(id)'],
    ['|count(id)', 'count(id|)'],
    ["|where id in (\n  select (1)\n)", "where id in (\n  select (1)\n|)"],
    ['in (a, [b]|)', 'in |(a, [b])'],
    ['|select 1', null],
    ['|count(id', null],
]);

it('moves by paragraph', function () {
    [$text, $at] = cursorIn("sel|ect\nfrom\n\nwhere\nand\n\nlimit");

    expect(marked($text, Motions::paragraphForward($text, $at, 1)))->toBe("select\nfrom\n|\nwhere\nand\n\nlimit")
        ->and(marked($text, Motions::paragraphForward($text, $at, 2)))->toBe("select\nfrom\n\nwhere\nand\n|\nlimit")
        ->and(marked($text, Motions::paragraphForward($text, $at, 3)))->toBe("select\nfrom\n\nwhere\nand\n\nlimi|t");

    [$text, $at] = cursorIn("select\nfrom\n\nwhere\nan|d");

    expect(marked($text, Motions::paragraphBackward($text, $at, 1)))->toBe("select\nfrom\n|\nwhere\nand")
        ->and(marked($text, Motions::paragraphBackward($text, $at, 2)))->toBe("|select\nfrom\n\nwhere\nand");
});
