<?php

use App\Tui\Browser;
use App\Tui\QueryEditor;
use Illuminate\Support\Facades\Artisan;
use Laravel\Prompts\Key;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function vimOn(string $marked): Browser
{
    $browser = sqlPane('vim');
    $browser->editor->set(str_replace('|', '', $marked));
    $browser->editor->moveTo(mb_strpos($marked, '|'));

    return $browser;
}

function vimKeys(Browser $browser, string ...$keys): Browser
{
    foreach ($keys as $key) {
        foreach (mb_strlen($key) > 1 && ! str_starts_with($key, "\e") ? mb_str_split($key) : [$key] as $single) {
            $browser->emit('key', $single);
        }
    }

    return $browser;
}

function withCursor(Browser $browser): string
{
    $buffer = $browser->editor->buffer();
    $at = $browser->editor->cursor();

    return mb_substr($buffer, 0, $at).'|'.mb_substr($buffer, $at);
}

it('moves with motions and counts', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['|select name from fruit', 'w', 'select |name from fruit'],
    ['|select name from fruit', '3w', 'select name from |fruit'],
    ['select name from |fruit', 'b', 'select name |from fruit'],
    ['|select name from fruit', 'e', 'selec|t name from fruit'],
    ["a\nb\n|c", 'gg', "|a\nb\nc"],
    ["|a\nb\nc", 'G', "a\nb\n|c"],
    ["|a\nb\nc", '2G', "a\n|b\nc"],
    ["|a\nb\nc", '2gg', "a\n|b\nc"],
    ['  sel|ect', '^', '  |select'],
    ['|select a, b, c', 'f,', 'select a|, b, c'],
    ['|select a, b, c', 'f,;', 'select a, b|, c'],
    ['|select a, b, c', 'f,;,', 'select a|, b, c'],
    ['|select a, b, c', 't,', 'select |a, b, c'],
    ['|count(id)', '%', 'count(id|)'],
    ["|a\nb\n\nc", '}', "a\nb\n|\nc"],
    ["a\nb\n\n|c", '{', "a\nb\n|\nc"],
    ["|a\n  b", "\n", "a\n  |b"],
    ['|select', '$', 'selec|t'],
    ["|ab\nabcdef\nab", 'j$j', "ab\nabcdef\na|b"],
    ["abcd|ef\nab\nabcdef", 'jj', "abcdef\nab\nabcd|ef"],
    ['|select', '5l', 'selec|t'],
]);

it('deletes with d and a motion or text object', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['|select name from', 'dw', '|name from'],
    ['|select name from', 'd2w', '|from'],
    ['|select name from', '2dw', '|from'],
    ['|a b c d e f g', '2d3w', '|g'],
    ['|select name', 'de', '| name'],
    ['sel|ect name', 'd$', 'se|l'],
    ['sel|ect name', 'D', 'se|l'],
    ['select |name', 'db', '|name'],
    ["select |name\nfrom", 'dw', "select| \nfrom"],
    ["a\n|b\nc\nd", 'dj', "a\n|d"],
    ["a\nb\n|c", 'dk', '|a'],
    ["a\n|b\nc", 'dG', '|a'],
    ["a\n|b\nc", 'dgg', '|c'],
    ["a\n|b\nc\nd", '2dd', "a\n|d"],
    ["a\nb\n|c", 'dd', "a\n|b"],
    ['select |a, b, c', 'dt,', 'select |, b, c'],
    ['|select a, b', 'df,', '| b'],
    ['select na|me from', 'diw', 'select | from'],
    ['select na|me from', 'daw', 'select |from'],
    ['count(i|d)', 'di(', 'count(|)'],
    ['count(i|d)', 'da(', 'coun|t'],
    ['count(i|d)', 'dib', 'count(|)'],
    ["where name = 'ch|erry'", "di'", "where name = '|'"],
    ['select "na|me" from', 'da"', 'select |from'],
    ["a\n\n|b\n\nc", 'dap', "a\n\n|c"],
    ['select |name', 'dz', 'select |name'],
    ['|select a', 'd'.Key::ESCAPE.'w', 'select |a'],
]);

it('changes with c and leaves you in insert', function (string $from, string $keys, string $to) {
    $browser = vimKeys(vimOn($from), $keys);

    expect($browser->sqlMode)->toBe('insert')
        ->and(withCursor($browser))->toBe($to);
})->with([
    ['|select name', 'cw', '| name'],
    ['|select name', 'c2w', '|'],
    ['select na|me from', 'ciw', 'select | from'],
    ['count(i|d)', 'ci(', 'count(|)'],
    ["where x in (\n  sel|ect 1\n)", 'ci(', 'where x in (|)'],
    ['sel|ect name', 'C', 'sel|'],
    ['sel|ect name', 'c$', 'sel|'],
    ["a\n  b|b\nc", 'cc', "a\n  |\nc"],
    ["a\n  b|b\nc", 'S', "a\n  |\nc"],
    ['|select', 's', '|elect'],
    ['|select', '3s', '|ect'],
]);

it('edits with the shortcuts', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['|select', 'x', '|elect'],
    ['|select', '3x', '|ect'],
    ['selec|t', 'x', 'sele|c'],
    ['sel|ect', 'X', 'se|ect'],
    ['sel|ect', '2X', 's|ect'],
    ['|select', 'rS', '|Select'],
    ['|select', '3rx', 'xx|xect'],
    ['|select', '9rx', '|select'],
    ["|select\n  name\nfrom", 'J', "select| name\nfrom"],
    ["|select\n  name\nfrom", '3J', 'select name| from'],
    ["|select\n)", 'J', 'select|)'],
    ['|select', '~', 'S|elect'],
    ['|select', '3~', 'SEL|ect'],
    ["|a\nb", '>>', "  |a\nb"],
    ["|a\nb", '2>>', "  |a\n  b"],
    ["|a\nb", '>j', "  |a\n  b"],
    ["    |a\nb", '<<', "  |a\nb"],
    ["a\n\n|b", '>k', "a\n|\n  b"],
]);

it('yanks and puts', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['|select name', 'yw$p', 'select nameselect| '],
    ['|select name', 'ywP', 'select| select name'],
    ["|a\nb", 'yyp', "a\n|a\nb"],
    ["a\n|b", 'yyP', "a\n|b\nb"],
    ["|a\nb", 'yy2p', "a\n|a\na\nb"],
    ["|a\nb", 'Yjp', "a\nb\n|a"],
    ["|a\nb", 'ddp', "b\n|a"],
    ['|ab', 'xp', 'b|a'],
    ['sel|ect', 'yiwP', 'selec|tselect'],
    ['|select', 'p', '|select'],
]);

it('copies a yank to the system clipboard too', function () {
    $browser = vimKeys(vimOn('|select'), 'yiw');

    expect($browser->vim->register)->toBe('select')
        ->and($browser->status)->toContain('yanked');
});

it('undoes and redoes a change at a time', function () {
    $browser = vimKeys(vimOn('|select name from'), 'dw', 'dw');

    expect($browser->editor->buffer())->toBe('from');

    vimKeys($browser, 'u');

    expect($browser->editor->buffer())->toBe('name from');

    vimKeys($browser, 'u');

    expect($browser->editor->buffer())->toBe('select name from');

    vimKeys($browser, 'u');

    expect($browser->editor->buffer())->toBe('select name from')
        ->and($browser->status)->toContain('oldest');

    vimKeys($browser, QueryEditor::RUN);

    expect($browser->editor->buffer())->toBe('name from')
        ->and($browser->resultsFromQuery)->toBeFalse();

    vimKeys($browser, QueryEditor::RUN, QueryEditor::RUN);

    expect($browser->editor->buffer())->toBe('from')
        ->and($browser->status)->toContain('newest');
});

it('undoes a whole insert as one step', function () {
    $browser = vimKeys(vimOn('|'), 'i', 'select name', "\n", 'from fruit', Key::ESCAPE);

    expect($browser->editor->buffer())->toBe("select name\nfrom fruit");

    vimKeys($browser, 'u');

    expect($browser->editor->buffer())->toBe('');
});

it('drops the redo history after a new change', function () {
    $browser = vimKeys(vimOn('|a b c'), 'dw', 'u', 'x', QueryEditor::RUN);

    expect($browser->editor->buffer())->toBe(' b c');
});

it('repeats the last change with .', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['|a b c d', 'dw.', '|c d'],
    ['|a b c d e f', 'dw3.', '|e f'],
    ['|a b c d e', '2dw.', '|e'],
    ['|one two', 'ciwX'.Key::ESCAPE.'w.', 'X |X'],
    ["|a\nb", 'Ax'.Key::ESCAPE.'j.', "ax\nb|x"],
    ["|a\nb\nc", 'dd.', '|c'],
    ["|a\nb", 'oz'.Key::ESCAPE.'.', "a\nz\n|z\nb"],
    ['|abc', 'x.', '|c'],
    ['|a b', 'ywP.', 'aa|  a b'],
]);

it('leaves insert on the last character typed', function () {
    $browser = vimKeys(vimOn('|'), 'i', 'select', Key::ESCAPE);

    expect(withCursor($browser))->toBe('selec|t');

    vimKeys($browser, 'I', Key::ESCAPE);

    expect(withCursor($browser))->toBe('|select');
});

it('never sits past the last character in normal mode', function () {
    $browser = vimKeys(vimOn('|select'), 'A', Key::ESCAPE, 'l');

    expect(withCursor($browser))->toBe('selec|t');
});

it('deletes a word or the line so far in insert mode', function () {
    $browser = vimKeys(vimOn('|'), 'i', 'select name', "\x17");

    expect($browser->editor->buffer())->toBe('select ');

    vimKeys($browser, 'from', "\x15");

    expect($browser->editor->buffer())->toBe('');
});

describe('visual', function () {
    it('selects characters and deletes them', function () {
        $browser = vimKeys(vimOn('|select name'), 'v');

        expect($browser->sqlMode)->toBe('visual')
            ->and(paintSql($browser))->toContain('SQL · VISUAL');

        vimKeys($browser, 'e', 'd');

        expect(withCursor($browser))->toBe('| name')
            ->and($browser->sqlMode)->toBe('normal');
    });

    it('selects lines and deletes them', function () {
        $browser = vimKeys(vimOn("a\n|b\nc\nd"), 'V');

        expect(paintSql($browser))->toContain('SQL · VISUAL LINE');

        vimKeys($browser, 'j', 'd');

        expect(withCursor($browser))->toBe("a\n|d");
    });

    it('selects with a text object and yanks', function () {
        $browser = vimKeys(vimOn('count(i|d)'), 'vi(', 'y');

        expect($browser->vim->register)->toBe('id')
            ->and(withCursor($browser))->toBe('count(|id)');
    });

    it('swaps ends with o', function () {
        $browser = vimKeys(vimOn('sel|ect name'), 'v', 'e', 'o', 'b', 'd');

        expect(withCursor($browser))->toBe('| name');
    });

    it('acts on the selection', function (string $from, string $keys, string $to) {
        expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
    })->with([
        ["|a\nb\nc", 'Vj>', "  |a\n  b\nc"],
        ["  |a\n  b", 'Vj<', "|a\nb"],
        ['|select', 'vll~', '|SELect'],
        ["|a\nb\nc", 'VjJ', "a| b\nc"],
        ['|select', 'vlx', '|lect'],
        ['|select name', 'vec', '|'.' name'],
    ]);

    it('switches between v and V, and leaves', function () {
        $browser = vimKeys(vimOn('|select'), 'v', 'V');

        expect($browser->sqlMode)->toBe('visual line');

        vimKeys($browser, 'v');

        expect($browser->sqlMode)->toBe('visual');

        vimKeys($browser, 'v');

        expect($browser->sqlMode)->toBe('normal');

        vimKeys($browser, 'V', Key::ESCAPE);

        expect($browser->sqlMode)->toBe('normal')
            ->and($browser->mode)->toBe('query');
    });

    it('runs just the selection with :r', function () {
        $browser = vimKeys(vimOn("select name from fruit\n|select name from fruit where qty > 8"), 'V', ':r', "\n");

        expect($browser->resultsFromQuery)->toBeTrue()
            ->and(array_column($browser->raw, 'name'))->toBe(['apple'])
            ->and($browser->sqlMode)->toBe('normal')
            ->and($browser->editor->buffer())->toContain("\n");
    });
});

describe(':r', function () {
    it('runs the query from the sql editor', function () {
        $browser = vimKeys(vimOn('|select * from fruit where qty > 8'), ':');

        vimKeys($browser, 'r');

        expect(paintSql($browser))->toContain('runs the query');

        vimKeys($browser, "\n");

        expect($browser->resultsFromQuery)->toBeTrue()
            ->and($browser->raw)->toHaveCount(1)
            ->and($browser->mode)->toBe('query');
    });

    it('runs the query with :run too', function () {
        $browser = vimKeys(vimOn('|select * from fruit where qty > 8'), ':run', "\n");

        expect($browser->raw)->toHaveCount(1);
    });

    it('still reloads away from the sql editor', function () {
        $browser = vimKeys(vimOn('|select * from fruit where qty > 8'), Key::ESCAPE, ':r');

        expect(paintSql($browser))->toContain('reloads');

        vimKeys($browser, "\n");

        expect($browser->resultsFromQuery)->toBeFalse();
    });

    it('shows :r in the footer instead of ctrl+r', function () {
        expect(paintSql(vimOn('|select 1')))->toContain(':r')->not->toContain('ctrl+r');
    });
});

it('keeps the indentation on a new line', function (string $from, string $keys, string $to) {
    expect(withCursor(vimKeys(vimOn($from), $keys)))->toBe($to);
})->with([
    ['  |where a = 1', 'A'."\n".'and', "  where a = 1\n  and|"],
    ['  |where a = 1', 'o'.'and', "  where a = 1\n  and|"],
    ['  |where a = 1', 'O'.'and', "  and|\n  where a = 1"],
    ['|where a = 1', 'o'.'and', "where a = 1\nand|"],
]);

it('runs the statement under the cursor with :r', function () {
    $browser = vimKeys(vimOn("select * from fruit where qty > 8;\nselect * from |fruit"), ':r', "\n");

    expect($browser->raw)->toHaveCount(3)
        ->and($browser->status)->toContain('statement 2 of 2');

    vimKeys($browser, 'gg', ':r', "\n");

    expect($browser->raw)->toHaveCount(1)
        ->and($browser->status)->toContain('statement 1 of 2');
});
