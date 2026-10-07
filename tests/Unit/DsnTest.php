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

it('reads a TablePlus url that goes over ssh', function () {
    $parsed = Dsn::parse('mysql+ssh://forge@203.0.113.7:2222/app:s3cret@127.0.0.1:3307/shop?statusColor=6D0000&env=production&name=ancient-jakarta&tLSMode=0&usePrivateKey=true');

    expect($parsed)->toMatchArray([
        'name' => 'ancient-jakarta',
        'driver' => 'mysql',
        'host' => '127.0.0.1',
        'port' => 3307,
        'database' => 'shop',
        'username' => 'app',
        'password' => 's3cret',
        'tag' => 'production',
        'ssh_host' => '203.0.113.7',
        'ssh_port' => 2222,
        'ssh_user' => 'forge',
    ]);
});

it('names an unnamed ssh connection after the server it goes through', function () {
    expect(Dsn::parse('postgresql+ssh://deploy@db.example.com/app:pw@localhost/orders')['name'])->toBe('orders on db.example.com')
        ->and(Dsn::parse('mysql+ssh://forge@203.0.113.7/forge:pw@127.0.0.1')['name'])->toBe('203.0.113.7');
});

it('turns a TablePlus environment into a tag', function (string $environment, ?string $tag) {
    expect(Dsn::parse("mysql://u:p@h/db?env={$environment}")['tag'] ?? null)->toBe($tag);
})->with([
    ['production', 'production'],
    ['staging', 'staging'],
    ['development', 'dev'],
    ['testing', 'dev'],
    ['local', 'local'],
    ['other', null],
]);

it('refuses an ssh url with nothing after the server', function () {
    expect(Dsn::parse('mysql+ssh://forge@203.0.113.7'))->toBeNull();
});
