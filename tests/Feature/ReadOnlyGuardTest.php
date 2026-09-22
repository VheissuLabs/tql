<?php

use App\Database\QueryRunner;

dataset('read only statements', [
    'select * from users',
    'SELECT 1',
    '  select 1',
    "-- comment\nselect 1",
    '/* block */ select 1',
    'with x as (select 1) select * from x',
    'show tables',
    'explain select 1',
    'pragma table_info(users)',
    'select 1;',
]);

dataset('write statements', [
    'delete from users',
    'DROP TABLE users',
    'update users set a = 1',
    'insert into users values (1)',
    'truncate users',
    'alter table users add column x int',
    'create table x (id int)',
    'grant all on *.* to bad',
    '',
]);

dataset('stacked statements', [
    'select 1; drop table users',
    'select 1; delete from users;',
    'SeLeCt 1; DrOp TaBlE x',
]);

it('allows read-only statements', function (string $sql) {
    expect(app(QueryRunner::class)->isReadOnly($sql))->toBeTrue();
})->with('read only statements');

it('rejects statements that write', function (string $sql) {
    expect(app(QueryRunner::class)->isReadOnly($sql))->toBeFalse();
})->with('write statements');

it('rejects a write smuggled after a select', function (string $sql) {
    expect(app(QueryRunner::class)->isReadOnly($sql))->toBeFalse();
})->with('stacked statements');
