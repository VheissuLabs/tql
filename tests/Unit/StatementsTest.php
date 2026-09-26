<?php

use App\Tui\Statements;

function statementAt(string $marked): ?array
{
    $at = mb_strpos($marked, '|');

    return Statements::at(str_replace('|', '', $marked), $at);
}

it('takes the whole buffer when there is one statement', function () {
    expect(statementAt('select * |from fruit'))->toMatchArray(['sql' => 'select * from fruit', 'number' => 1, 'of' => 1]);
});

it('takes the statement the cursor is in', function (string $marked, string $sql, int $number) {
    expect(statementAt($marked))->toMatchArray(['sql' => $sql, 'number' => $number, 'of' => 2]);
})->with([
    ['sel|ect 1; select 2', 'select 1', 1],
    ['select 1; sel|ect 2', 'select 2', 2],
    ['select 1|; select 2', 'select 1', 1],
    ["select 1;\nselect 2;|", 'select 2', 2],
    ["select 1;|\n\nselect 2;", 'select 1', 1],
    ["select 1;\n\n|select 2;\n", 'select 2', 2],
    ["select 1;\n|\n\nselect 2", 'select 2', 2],
]);

it('does not split on a semicolon that is quoted or commented', function (string $sql, string $first) {
    expect(Statements::at($sql, 0)['sql'])->toBe($first);
})->with([
    ["select ';' as x; select 2", "select ';' as x"],
    ["select 'it''s;' as x; select 2", "select 'it''s;' as x"],
    ['select ";" from t; select 2', 'select ";" from t'],
    ["select 1 -- one; two\n, 2; select 3", "select 1 -- one; two\n, 2"],
    ["select /* a;\nb */ 1; select 2", "select /* a;\nb */ 1"],
    ['create function f() returns int as $$ begin; return 1; end $$ language plpgsql; select 2', 'create function f() returns int as $$ begin; return 1; end $$ language plpgsql'],
    ['select $tag$ a; b $tag$; select 2', 'select $tag$ a; b $tag$'],
]);

it('finds nothing to run in a blank buffer', function () {
    expect(Statements::at("  \n ", 1))->toBeNull()
        ->and(Statements::at(';;', 1))->toBeNull();
});
