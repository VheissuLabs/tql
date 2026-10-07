<?php

use App\Dump\StatementReader;

it('splits on semicolons outside quotes and drops comments', function () {
    $sql = "-- tql export\n-- table: a; b\n\ninsert into t values ('a;b', \"c;d\", `e;f`);\n\ninsert into t values (1);";

    expect((new StatementReader(false))->split($sql))->toBe([
        "insert into t values ('a;b', \"c;d\", `e;f`)",
        'insert into t values (1)',
    ]);
});

it('reads a doubled quote as part of the string', function () {
    expect((new StatementReader(false))->split("insert into t values ('it''s; fine');select 1"))
        ->toBe(["insert into t values ('it''s; fine')", 'select 1']);
});

it('honours backslash escapes only where the database does', function () {
    $sql = "insert into t values ('a\\'; b');select 2";

    expect((new StatementReader(true))->split($sql))->toBe(["insert into t values ('a\\'; b')", 'select 2'])
        ->and((new StatementReader(false))->split($sql))->toBe(["insert into t values ('a\\'", "b');select 2"]);
});

it('keeps a double dash inside a string', function () {
    expect((new StatementReader(false))->split("insert into t values ('a -- b');"))
        ->toBe(["insert into t values ('a -- b')"]);
});
