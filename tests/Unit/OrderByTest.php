<?php

use App\Database\OrderBy;

it('appends an order by to a plain select', function () {
    expect(OrderBy::apply('select * from events', 'name', 'asc', '"name"'))
        ->toBe('select * from events order by "name" asc');
});

it('puts the clause before a limit', function () {
    expect(OrderBy::apply('select * from events limit 50 offset 0', 'name', 'desc', '"name"'))
        ->toBe('select * from events order by "name" desc limit 50 offset 0');
});

it('replaces an order by that is already there', function () {
    expect(OrderBy::apply('select * from events order by id desc limit 10', 'name', 'asc', '"name"'))
        ->toBe('select * from events order by "name" asc limit 10');
});

it('keeps a where clause', function () {
    expect(OrderBy::apply("select * from events where name = 'a'", 'id', 'asc', '"id"'))
        ->toBe('select * from events where name = \'a\' order by "id" asc');
});

it('strips the clause when the sort is cleared', function () {
    expect(OrderBy::apply('select * from events order by id desc limit 10', null, 'asc', ''))
        ->toBe('select * from events limit 10');
});

it('refuses anything it cannot rewrite safely', function (string $statement) {
    expect(OrderBy::apply($statement, 'name', 'asc', '"name"'))->toBeNull();
})->with([
    'select a from x union select b from y',
    'select * from x; drop table y',
    'update events set name = 1',
    'with t as (select 1) select * from t',
]);
