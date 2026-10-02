<?php

use App\Database\ConnectionManager;
use App\Models\Connection;

function sqlServer(?string $sslMode): Connection
{
    return new Connection([
        'name' => 'mssql',
        'driver' => 'sqlsrv',
        'host' => 'db.example.com',
        'port' => 1433,
        'username' => 'sa',
        'password' => 'secret',
        'database' => 'shop',
        'ssl_mode' => $sslMode,
    ]);
}

it('encrypts a SQL Server connection the way the same SSL mode does for Postgres', function (?string $mode, string $encrypt, string $trust) {
    $config = sqlServer($mode)->toLaravelConfig();

    expect($config['encrypt'])->toBe($encrypt)
        ->and($config['trust_server_certificate'])->toBe($trust)
        ->and($config)->not->toHaveKey('options');
})->with([
    'nothing set' => [null, 'yes', 'true'],
    'disable' => ['disable', 'no', 'true'],
    'prefer' => ['prefer', 'yes', 'true'],
    'require' => ['require', 'yes', 'true'],
    'verify-ca' => ['verify-ca', 'yes', 'false'],
    'verify-full' => ['verify-full', 'yes', 'false'],
]);

it('says how to install Microsoft\'s ODBC driver when SQL Server cannot find it', function () {
    $missing = 'SQLSTATE[IMSSP]: This extension requires the Microsoft ODBC Driver for SQL Server to communicate with SQL Server.';

    expect(ConnectionManager::explain($missing))->toContain(PHP_OS_FAMILY === 'Darwin' ? 'brew install msodbcsql18' : 'learn.microsoft.com')
        ->and(ConnectionManager::explain('Access denied for user'))->toBe('Access denied for user');
});
