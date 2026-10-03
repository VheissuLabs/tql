<?php

use App\Database\ConnectionManager;
use App\Support\OpenSslForSqlServer;

function needsOpenSsl3(array $overrides = []): bool
{
    $situation = array_merge([
        'os' => 'Darwin',
        'sqlServer' => true,
        'driverRegistered' => true,
        'openSsl3' => true,
        'defaultOpenSsl' => '/opt/homebrew/Cellar/openssl@4/4.0.3/lib/libssl.4.dylib',
        'canRelaunch' => true,
    ], $overrides);

    return OpenSslForSqlServer::needed(...$situation);
}

it('relaunches on a Mac whose default OpenSSL is one Microsoft\'s driver cannot use', function () {
    expect(needsOpenSsl3())->toBeTrue();
});

it('leaves everyone else alone', function (array $overrides) {
    expect(needsOpenSsl3($overrides))->toBeFalse();
})->with([
    'not a Mac' => [['os' => 'Linux']],
    'no SQL Server in this build' => [['sqlServer' => false]],
    'no Microsoft driver installed' => [['driverRegistered' => false]],
    'no OpenSSL 3 to point at' => [['openSsl3' => false]],
    'the default is already OpenSSL 3' => [['defaultOpenSsl' => '/opt/homebrew/Cellar/openssl@3/3.6.0/lib/libssl.3.dylib']],
    'cannot relaunch' => [['canRelaunch' => false]],
]);

it('says to install OpenSSL 3 when Microsoft\'s driver cannot load OpenSSL', function () {
    $message = 'SQLSTATE[08001]: [Microsoft][ODBC Driver 18 for SQL Server]SSL Provider: [OpenSSL library could not be loaded, make sure OpenSSL 1.0, 1.1, or 3.0 is installed]';

    expect(ConnectionManager::explain($message))->toContain('brew install openssl@3');
});
