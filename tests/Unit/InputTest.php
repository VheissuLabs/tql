<?php

use App\Tui\Input;

it('keeps a pasted burst of characters', function () {
    expect(Input::text('postgres.internal'))->toBe('postgres.internal');
});

it('flattens newlines in a single line field', function () {
    expect(Input::text("one\ntwo"))->toBe('one two')
        ->and(Input::text("one\r\ntwo"))->toBe('one two');
});

it('keeps newlines where they mean something', function () {
    expect(Input::text("select *\nfrom users", newlines: true))->toBe("select *\nfrom users")
        ->and(Input::text("select *\r\nfrom users", newlines: true))->toBe("select *\nfrom users");
});

it('ignores a key sequence', function (string $key) {
    expect(Input::text($key))->toBe('');
})->with(["\e[A", "\e[B", "\e[1;2A", "\e[<0;10;5M", "\e"]);

it('drops control characters', function () {
    expect(Input::text("a\x00b\x07c"))->toBe('abc');
});

it('keeps unicode', function () {
    expect(Input::text('café ünï 日本'))->toBe('café ünï 日本');
});

it('keeps a single character', function () {
    expect(Input::text('x'))->toBe('x')
        ->and(Input::text(' '))->toBe(' ');
});
