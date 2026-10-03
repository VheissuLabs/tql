<?php

use App\Database\QueryRunner;

it('pages with limit and offset where the database has them', function (string $driver) {
    expect(QueryRunner::page($driver, 'select * from "fruit" order by "name" asc', 50, 100))
        ->toBe('select * from "fruit" order by "name" asc limit 50 offset 100');
})->with(['mysql', 'mariadb', 'pgsql', 'sqlite']);

it('pages SQL Server with offset and fetch, giving it the order it insists on', function () {
    expect(QueryRunner::page('sqlsrv', 'select * from [fruit]', 50, 0))
        ->toBe('select * from [fruit] order by (select null) offset 0 rows fetch next 50 rows only')
        ->and(QueryRunner::page('sqlsrv', 'select * from [fruit] order by [name] desc', 50, 100))
        ->toBe('select * from [fruit] order by [name] desc offset 100 rows fetch next 50 rows only');
});

it('inserts a row of nothing but defaults the way each database spells it', function (string $driver, string $spelled) {
    expect(QueryRunner::defaultsOnly($driver, '"fruit"'))->toBe("insert into \"fruit\" {$spelled}");
})->with([
    ['mysql', '() values ()'],
    ['mariadb', '() values ()'],
    ['sqlite', 'default values'],
    ['pgsql', 'default values'],
    ['sqlsrv', 'default values'],
]);
