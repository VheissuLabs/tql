<?php

use App\Tui\Layout;
use App\Tui\QueryEditor;
use Illuminate\Support\Facades\Artisan;
use Laravel\Prompts\Key;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false, 'tql.ui.sql_editor' => null]);
});

it('reads the editor style from the config and falls back to simple', function (?string $configured, string $style) {
    config(['tql.ui.sql_editor' => $configured]);

    expect(Layout::sqlEditor())->toBe($style);
})->with([
    [null, 'simple'],
    ['simple', 'simple'],
    ['vim', 'vim'],
    ['VIM', 'vim'],
    ['emacs', 'simple'],
]);

describe('simple', function () {
    it('adds a line on enter instead of running', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fruit');
        $browser->emit('key', Key::ENTER);
        typeSql($browser, 'where qty > 1');

        expect($browser->editor->buffer())->toBe("select * from fruit\nwhere qty > 1")
            ->and($browser->resultsFromQuery)->toBeFalse();
    });

    it('runs the query on ctrl+r', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fruit where qty > 1');
        $browser->emit('key', QueryEditor::RUN);

        expect($browser->resultsFromQuery)->toBeTrue()
            ->and(array_column($browser->raw, 'name'))->toBe(['cherry', 'apple']);
    });

    it('indents on tab and stays in the pane', function () {
        $browser = sqlPane();

        typeSql($browser, 'select *');
        $browser->emit('key', Key::ENTER);
        $browser->emit('key', Key::TAB);
        typeSql($browser, 'from fruit');

        expect($browser->mode)->toBe('query')
            ->and($browser->editor->buffer())->toBe("select *\n  from fruit");
    });

    it('outdents on shift+tab and stays in the pane', function () {
        $browser = sqlPane();
        $browser->editor->set("select *\n    from fruit");

        $browser->emit('key', Key::SHIFT_TAB);

        expect($browser->mode)->toBe('query')
            ->and($browser->editor->buffer())->toBe("select *\n  from fruit")
            ->and($browser->editor->cursorColumn())->toBe(12);
    });

    it('outdents only the spaces a line has', function () {
        $browser = sqlPane();
        $browser->editor->set(' from fruit');
        $browser->editor->toStart();

        $browser->emit('key', Key::SHIFT_TAB);
        $browser->emit('key', Key::SHIFT_TAB);

        expect($browser->editor->buffer())->toBe('from fruit')
            ->and($browser->editor->cursorColumn())->toBe(0);
    });

    it('returns to the grid on escape', function () {
        $browser = sqlPane();

        $browser->emit('key', Key::ESCAPE);

        expect($browser->mode)->toBe('browse');
    });

    it('offers ctrl+r in the footer', function () {
        $browser = sqlPane();

        expect(paintSql($browser))->toContain('New line')->toContain('Run')
            ->and(paintSql($browser))->not->toContain('NORMAL');
    });
});

describe('vim', function () {
    it('opens in normal mode', function () {
        $browser = sqlPane('vim');

        expect($browser->sqlMode)->toBe('normal')
            ->and(paintSql($browser))->toContain('SQL · NORMAL');
    });

    it('does not type in normal mode', function () {
        $browser = sqlPane('vim');
        $browser->editor->set('select 1');

        $browser->emit('key', 'z');

        expect($browser->editor->buffer())->toBe('select 1');
    });

    it('types after i, adds lines on enter and stays out of the grid', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        typeSql($browser, 'select * from fruit');
        $browser->emit('key', Key::ENTER);
        typeSql($browser, 'where qty > 1');

        expect($browser->sqlMode)->toBe('insert')
            ->and(paintSql($browser))->toContain('SQL · INSERT')
            ->and($browser->editor->buffer())->toBe("select * from fruit\nwhere qty > 1")
            ->and($browser->resultsFromQuery)->toBeFalse();
    });

    it('indents on tab in insert mode', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        $browser->emit('key', Key::TAB);

        expect($browser->mode)->toBe('query')
            ->and($browser->editor->buffer())->toBe('  ');
    });

    it('goes from insert to normal to the grid on escape', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        $browser->emit('key', Key::ESCAPE);

        expect($browser->mode)->toBe('query')
            ->and($browser->sqlMode)->toBe('normal');

        $browser->emit('key', Key::ESCAPE);

        expect($browser->mode)->toBe('browse');
    });

    it('outdents on shift+tab in insert mode', function () {
        $browser = sqlPane('vim');
        $browser->editor->set('  from fruit');

        $browser->emit('key', 'i');
        $browser->emit('key', Key::SHIFT_TAB);

        expect($browser->mode)->toBe('query')
            ->and($browser->editor->buffer())->toBe('from fruit');
    });

    it('leaves the pane on tab in normal mode', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', Key::TAB);

        expect($browser->mode)->toBe('browse');
    });

    it('never runs on enter in normal mode', function () {
        $browser = sqlPane('vim');
        $browser->editor->set("select * from fruit\nwhere qty > 1");
        $browser->editor->toStart();

        $browser->emit('key', Key::ENTER);

        expect($browser->resultsFromQuery)->toBeFalse()
            ->and($browser->editor->cursorLine())->toBe(1);
    });

    it('does not run the query on ctrl+r, which is redo', function (string $mode) {
        $browser = sqlPane('vim');
        $browser->editor->set('select * from fruit where qty > 8');

        if ($mode === 'insert') {
            $browser->emit('key', 'i');
        }

        $browser->emit('key', QueryEditor::RUN);

        expect($browser->resultsFromQuery)->toBeFalse();
    })->with(['normal', 'insert']);

    it('comes back in normal mode after leaving from insert', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        $browser->emit('key', "\e2");

        expect($browser->mode)->toBe('browse');

        $browser->emit('key', 's');

        expect($browser->sqlMode)->toBe('normal');
    });

    it('moves with h j k l, 0, $, gg and G', function () {
        $browser = sqlPane('vim');
        $browser->editor->set("select *\nfrom fruit\nwhere qty > 1");
        $browser->editor->toStart();

        $browser->emit('key', 'j');
        $browser->emit('key', 'l');
        $browser->emit('key', 'l');

        expect([$browser->editor->cursorLine(), $browser->editor->cursorColumn()])->toBe([1, 2]);

        $browser->emit('key', 'h');
        $browser->emit('key', 'k');

        expect([$browser->editor->cursorLine(), $browser->editor->cursorColumn()])->toBe([0, 1]);

        $browser->emit('key', '$');

        expect($browser->editor->cursorColumn())->toBe(7);

        $browser->emit('key', '0');

        expect($browser->editor->cursorColumn())->toBe(0);

        $browser->emit('key', 'G');

        expect($browser->editor->cursorLine())->toBe(2);

        $browser->emit('key', 'g');
        $browser->emit('key', 'g');

        expect($browser->editor->cursorLine())->toBe(0);
    });

    it('deletes a character with x', function () {
        $browser = sqlPane('vim');
        $browser->editor->set('select 1');
        $browser->editor->toStart();

        $browser->emit('key', 'x');

        expect($browser->editor->buffer())->toBe('elect 1');
    });

    it('deletes a line with dd', function () {
        $browser = sqlPane('vim');
        $browser->editor->set("select *\nfrom fruit\nwhere qty > 1");
        $browser->editor->toLine(1);

        $browser->emit('key', 'd');
        $browser->emit('key', 'd');

        expect($browser->editor->buffer())->toBe("select *\nwhere qty > 1")
            ->and($browser->editor->cursorLine())->toBe(1);
    });

    it('deletes the only line with dd', function () {
        $browser = sqlPane('vim');
        $browser->editor->set('select 1');

        $browser->emit('key', 'd');
        $browser->emit('key', 'd');

        expect($browser->editor->buffer())->toBe('');
    });

    it('enters insert where a, A, I, o and O say', function (string $key, string $expected) {
        $browser = sqlPane('vim');
        $browser->editor->set("select *\nfrom fruit");
        $browser->editor->toLineColumn(0, 2);

        $browser->emit('key', $key);
        $browser->emit('key', '|');

        expect($browser->sqlMode)->toBe('insert')
            ->and($browser->editor->buffer())->toBe($expected);
    })->with([
        ['i', "se|lect *\nfrom fruit"],
        ['a', "sel|ect *\nfrom fruit"],
        ['I', "|select *\nfrom fruit"],
        ['A', "select *|\nfrom fruit"],
        ['o', "select *\n|\nfrom fruit"],
        ['O', "|\nselect *\nfrom fruit"],
    ]);
});
