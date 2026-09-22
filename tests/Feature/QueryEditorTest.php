<?php

use App\Tui\QueryEditor;
use Laravel\Prompts\Key;

function typed(string $text): QueryEditor
{
    $editor = new QueryEditor;

    foreach (preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) as $char) {
        $editor->handle($char);
    }

    return $editor;
}

it('types characters into the buffer', function () {
    expect(typed('select 1')->buffer())->toBe('select 1');
});

it('starts empty', function () {
    expect((new QueryEditor)->isEmpty())->toBeTrue()
        ->and(typed('   ')->isEmpty())->toBeTrue()
        ->and(typed('select 1')->isEmpty())->toBeFalse();
});

it('inserts newlines with enter', function () {
    $editor = typed('select 1');
    $editor->handle(Key::ENTER);
    $editor->handle('f');

    expect($editor->buffer())->toBe("select 1\nf")
        ->and($editor->lines())->toBe(['select 1', 'f']);
});

it('deletes backwards', function () {
    $editor = typed('select');
    $editor->handle(Key::BACKSPACE);

    expect($editor->buffer())->toBe('selec');
});

it('does not delete past the start', function () {
    $editor = new QueryEditor;
    $editor->handle(Key::BACKSPACE);

    expect($editor->buffer())->toBe('');
});

it('inserts at the cursor after moving left', function () {
    $editor = typed('sect');
    $editor->handle(Key::LEFT);
    $editor->handle(Key::LEFT);
    $editor->handle('l');
    $editor->handle('e');

    expect($editor->buffer())->toBe('select');
});

it('tracks the cursor line and column', function () {
    $editor = typed('ab');
    $editor->handle(Key::ENTER);
    $editor->handle('c');

    expect($editor->cursorLine())->toBe(1)
        ->and($editor->cursorColumn())->toBe(1);
});

it('moves between lines keeping the column where possible', function () {
    $editor = new QueryEditor;
    $editor->set("select *\nfrom tracks");
    $editor->handle(Key::HOME[0]);
    $editor->handle(Key::UP);

    expect($editor->cursorLine())->toBe(0)
        ->and($editor->cursorColumn())->toBe(0);
});

it('ignores up at the first line and down at the last', function () {
    $editor = typed('one');
    $editor->handle(Key::UP);
    $editor->handle(Key::DOWN);

    expect($editor->cursorLine())->toBe(0)
        ->and($editor->buffer())->toBe('one');
});

it('accepts every escape sequence for home', function () {
    foreach (Key::HOME as $sequence) {
        $editor = new QueryEditor;
        $editor->set("select *\nfrom tracks");
        $editor->handle($sequence);

        expect($editor->cursorColumn())->toBe(0);
    }
});

it('jumps to line start and end', function () {
    $editor = new QueryEditor;
    $editor->set("select *\nfrom tracks");
    $editor->handle(Key::HOME[0]);

    expect($editor->cursorColumn())->toBe(0);

    $editor->handle(Key::END[0]);

    expect($editor->cursorColumn())->toBe(mb_strlen('from tracks'));
});

it('indents with tab', function () {
    $editor = new QueryEditor;
    $editor->handle(Key::TAB);
    $editor->handle('x');

    expect($editor->buffer())->toBe('  x');
});

it('ignores control characters', function () {
    $editor = typed('select');
    $editor->handle("\x00");

    expect($editor->buffer())->toBe('select');
});
