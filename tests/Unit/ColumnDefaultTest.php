<?php

use App\Database\ColumnDefault;

it('reads a column default the way each driver reports it', function (mixed $default, string $driver, string $kind, ?string $value) {
    expect(ColumnDefault::read($default, $driver))->toBe([$kind, $value]);
})->with([
    'no default' => [null, 'sqlite', ColumnDefault::NONE, null],
    'default null' => ['NULL', 'sqlite', ColumnDefault::NONE, null],
    'a cast null' => ['NULL::character varying', 'pgsql', ColumnDefault::NONE, null],
    'a number' => ['3', 'sqlite', ColumnDefault::VALUE, '3'],
    'a decimal' => ['4.99', 'mysql', ColumnDefault::VALUE, '4.99'],
    'a quoted string' => ["'G'", 'sqlite', ColumnDefault::VALUE, 'G'],
    'an escaped quote' => ["'it''s'", 'sqlite', ColumnDefault::VALUE, "it's"],
    'a double-quoted sqlite string' => ['"untitled"', 'sqlite', ColumnDefault::VALUE, 'untitled'],
    'a postgres cast' => ["'G'::character varying", 'pgsql', ColumnDefault::VALUE, 'G'],
    'a bare mysql string' => ['G', 'mysql', ColumnDefault::VALUE, 'G'],
    'a sql server string' => ["(N'G')", 'sqlsrv', ColumnDefault::VALUE, 'G'],
    'a sql server number' => ['((0))', 'sqlsrv', ColumnDefault::VALUE, '0'],
    'a boolean' => ['true', 'pgsql', ColumnDefault::VALUE, 'true'],
    'current timestamp' => ['CURRENT_TIMESTAMP', 'sqlite', ColumnDefault::NOW, 'now()'],
    'mariadb current timestamp' => ['current_timestamp()', 'mysql', ColumnDefault::NOW, 'now()'],
    'postgres now' => ['now()', 'pgsql', ColumnDefault::NOW, 'now()'],
    'sqlite datetime now' => ["(datetime('now'))", 'sqlite', ColumnDefault::NOW, 'now()'],
    'sql server getdate' => ['(getdate())', 'sqlsrv', ColumnDefault::NOW, 'now()'],
    'a sequence' => ["nextval('film_film_id_seq'::regclass)", 'pgsql', ColumnDefault::EXPRESSION, "nextval('film_film_id_seq'::regclass)"],
    'a uuid' => ['gen_random_uuid()', 'pgsql', ColumnDefault::EXPRESSION, 'gen_random_uuid()'],
    'a mysql expression' => ['(uuid())', 'mysql', ColumnDefault::EXPRESSION, 'uuid()'],
    'parentheses that do not wrap it all' => ['(1) + (2)', 'sqlite', ColumnDefault::EXPRESSION, '(1) + (2)'],
]);
