<?php

use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\Styler;
use App\Tui\QueryEditor;
use App\Tui\Vim\Range;

function selectionStyle(): Styler
{
    $same = fn (string $text) => $text;

    return new Styler($same, $same, $same, $same, fn (string $text, int $width) => mb_substr($text, 0, $width), fn (string $name, string $text) => match ($name) {
        'cursor' => "[{$text}]",
        'selection' => "{{$text}}",
        default => $text,
    });
}

function selected(string $sql, int $cursor, Range $selection): array
{
    $editor = new QueryEditor;
    $editor->set($sql);
    $editor->moveTo($cursor);

    return (new EditorIsland($editor, true, null, selectionStyle(), $selection))->content(30, 10);
}

it('paints a character selection, with the cursor at its free end', function () {
    expect(selected('select name', 3, new Range(0, 4)))->toBe(['{s}{e}{l}[e]ct name']);
});

it('paints a selection that runs across lines', function () {
    expect(selected("select\nname\nfrom", 9, new Range(3, 10)))->toBe(['sel{e}{c}{t}', '{n}{a}[m]e', 'from']);
});

it('marks an empty line inside a line selection', function () {
    expect(selected("a\n\nb", 0, new Range(0, 4, true)))->toBe(['[a]', '{ }', '{b}']);
});
