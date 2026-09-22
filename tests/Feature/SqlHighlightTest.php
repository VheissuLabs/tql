<?php

use App\Tui\Sql;

it('never drops characters', function (string $line) {
    expect(implode('', array_column(Sql::tokenise($line), 1)))->toBe($line);
})->with([
    'select * from "settings" limit 100 offset 0',
    "select name from tracks where Composer like '%AC/DC%'",
    'update t set a = 1 where id = 42',
    '-- just a comment',
    '/* block */ select 1',
    '',
    '   ',
    'SELECT  *   FROM  t',
    "insert into t (a, b) values ('x', 2)",
    'select "quoted col", `backtick`, [bracket] from t',
]);

it('marks keywords regardless of case', function (string $word) {
    $types = array_column(Sql::tokenise($word.' x'), 0);

    expect($types)->toContain('keyword');
})->with(['select', 'SELECT', 'SeLeCt', 'from', 'WHERE', 'limit', 'join']);

it('does not mark words that merely contain a keyword', function () {
    $types = array_column(Sql::tokenise('selected preselect fromage'), 0);

    expect($types)->not->toContain('keyword');
});

it('marks strings, numbers, identifiers and comments', function () {
    $tokens = Sql::tokenise('select "col" from t where a = 42 and b = \'x\' -- note');
    $types = array_column($tokens, 0);

    expect($types)->toContain('identifier')
        ->and($types)->toContain('number')
        ->and($types)->toContain('string')
        ->and($types)->toContain('comment');
});

it('keeps a string containing a keyword as a string', function () {
    $tokens = Sql::tokenise("select 'select from where'");

    $strings = array_values(array_filter($tokens, fn ($t) => $t[0] === 'string'));

    expect($strings)->toHaveCount(1)
        ->and($strings[0][1])->toBe("'select from where'");
});

it('treats a comment to end of line as one token', function () {
    $tokens = Sql::tokenise('select 1 -- select from where');

    $comments = array_values(array_filter($tokens, fn ($t) => $t[0] === 'comment'));

    expect($comments)->toHaveCount(1)
        ->and($comments[0][1])->toBe('-- select from where');
});
