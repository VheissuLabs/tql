<?php

use Laravel\Prompts\Key;

beforeEach(function () {
    config(['tql.ui.sql_complete' => true]);
});

describe('simple', function () {
    it('offers the tables after from and takes one with tab', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fr');

        expect($browser->completion)->not->toBeNull()
            ->and(paintSql($browser))->toContain('fruit');

        $browser->emit('key', Key::TAB);

        expect($browser->editor->buffer())->toBe('select * from fruit')
            ->and($browser->completion)->toBeNull();
    });

    it('offers the columns of the open table', function () {
        $browser = sqlPane();

        typeSql($browser, 'select na');
        $browser->emit('key', Key::TAB);

        expect($browser->editor->buffer())->toBe('select name');
    });

    it('moves through the list with the arrows and ctrl+n', function (string $down) {
        $browser = sqlPane();

        typeSql($browser, 'select * from fruit where n');
        $browser->emit('key', $down);
        $browser->emit('key', Key::TAB);

        expect($browser->editor->buffer())->toBe('select * from fruit where not');
    })->with([Key::DOWN_ARROW, Key::CTRL_N]);

    it('still indents with tab when nothing is offered', function () {
        $browser = sqlPane();

        typeSql($browser, 'select ');
        $browser->emit('key', Key::TAB);

        expect($browser->editor->buffer())->toBe('select   ');
    });

    it('adds a line on enter instead of taking the suggestion', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::ENTER);

        expect($browser->editor->buffer())->toBe("select * from fr\n")
            ->and($browser->completion)->toBeNull();
    });

    it('closes the list on escape and leaves the editor on the next one', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::ESCAPE);

        expect($browser->completion)->toBeNull()
            ->and($browser->mode)->toBe('query');

        $browser->emit('key', Key::ESCAPE);

        expect($browser->mode)->toBe('browse');
    });

    it('undoes a completion in one step', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::TAB);
        $browser->emit('key', "\x1a");

        expect($browser->editor->buffer())->toBe('select * from fr');
    });

    it('offers nothing when sql_complete is off', function () {
        config(['tql.ui.sql_complete' => false]);

        $browser = sqlPane();

        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::TAB);

        expect($browser->completion)->toBeNull()
            ->and($browser->editor->buffer())->toBe('select * from fr  ');
    });

    it('draws the list under the cursor', function () {
        $browser = sqlPane();

        typeSql($browser, 'select * from fr');

        $lines = explode("\n", paintSql($browser));
        $typed = collect($lines)->search(fn (string $line) => str_contains($line, 'select * from fr'));
        $offered = collect($lines)->search(fn (string $line) => str_contains($line, 'fruit table'));

        expect($offered)->toBe($typed + 2)
            ->and(mb_strpos($lines[$offered], 'fruit'))->toBe(mb_strpos($lines[$typed], 'from fr') + 5);
    });
});

describe('vim', function () {
    it('completes in insert mode', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::TAB);

        expect($browser->editor->buffer())->toBe('select * from fruit')
            ->and($browser->sqlMode)->toBe('insert');
    });

    it('closes the list on the first escape and leaves insert on the second', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::ESCAPE);

        expect($browser->completion)->toBeNull()
            ->and($browser->sqlMode)->toBe('insert');

        $browser->emit('key', Key::ESCAPE);

        expect($browser->sqlMode)->toBe('normal');
    });

    it('offers nothing in normal mode', function () {
        $browser = sqlPane('vim');

        $browser->emit('key', 'i');
        typeSql($browser, 'select * from fr');
        $browser->emit('key', Key::ESCAPE);
        $browser->emit('key', Key::ESCAPE);
        $browser->emit('key', 'x');

        expect($browser->completion)->toBeNull();
    });
});
