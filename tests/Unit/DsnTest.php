<?php

use App\Database\Dsn;

it('parses a mysql connection string', function () {
    expect(Dsn::parse('mysql://alice:s3cret@db.example.com:3307/shop'))->toBe([
        'name' => 'shop on db.example.com',
        'driver' => 'mysql',
        'host' => 'db.example.com',
        'port' => 3307,
        'database' => 'shop',
        'username' => 'alice',
        'password' => 's3cret',
    ]);
});

it('takes the name from the query string', function () {
    $parsed = Dsn::parse('mysql://alice:pw@host/shop?name=Cloud%20-%20lunar');

    expect($parsed['name'])->toBe('Cloud - lunar');
});

it('fills in the default port for the driver', function (string $scheme, string $driver, int $port) {
    $parsed = Dsn::parse("{$scheme}://user:pw@host/db");

    expect($parsed['driver'])->toBe($driver)
        ->and($parsed['port'])->toBe($port);
})->with([
    ['mysql', 'mysql', 3306],
    ['mariadb', 'mysql', 3306],
    ['postgres', 'pgsql', 5432],
    ['postgresql', 'pgsql', 5432],
    ['pgsql', 'pgsql', 5432],
    ['sqlsrv', 'sqlsrv', 1433],
    ['mssql', 'sqlsrv', 1433],
]);

it('copes with no database in the path', function () {
    $parsed = Dsn::parse('mysql://user:pw@db.example.com?name=Cloud');

    expect($parsed['database'])->toBeNull()
        ->and($parsed['host'])->toBe('db.example.com')
        ->and($parsed['name'])->toBe('Cloud');
});

it('names it after the host when nothing else says', function () {
    expect(Dsn::parse('mysql://user:pw@db.example.com')['name'])->toBe('db.example.com');
});

it('decodes credentials that were percent encoded', function () {
    $parsed = Dsn::parse('pgsql://a%40b:p%2Fw%3A1@host/db');

    expect($parsed['username'])->toBe('a@b')
        ->and($parsed['password'])->toBe('p/w:1');
});

it('handles a sqlite url', function () {
    expect(Dsn::parse('sqlite:///tmp/app.sqlite'))->toBe([
        'name' => 'app.sqlite',
        'driver' => 'sqlite',
        'database' => '/tmp/app.sqlite',
    ]);
});

it('says no to a scheme it does not know', function (string $dsn) {
    expect(Dsn::parse($dsn))->toBeNull();
})->with([
    'redis://localhost',
    'https://example.com/db',
    'mongodb://host/db',
]);

it('knows a connection string from a file path', function () {
    expect(Dsn::looksLikeOne('mysql://host/db'))->toBeTrue()
        ->and(Dsn::looksLikeOne('./test.sqlite'))->toBeFalse()
        ->and(Dsn::looksLikeOne('/var/db/app.db'))->toBeFalse();
});
