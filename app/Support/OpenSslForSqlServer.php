<?php

namespace App\Support;

class OpenSslForSqlServer
{
    public const OPENSSL_3 = '/opt/homebrew/opt/openssl@3/lib';

    public const DEFAULT_OPENSSL = '/opt/homebrew/opt/openssl/lib/libssl.dylib';

    public const DRIVERS = '/opt/homebrew/etc/odbcinst.ini';

    public const MICROSOFT_DRIVER = 'ODBC Driver 18 for SQL Server';

    public const RELAUNCHED = 'TQL_OPENSSL_RELAUNCHED';

    public static function relaunchIfNeeded(): void
    {
        if (getenv(self::RELAUNCHED) !== false) {
            putenv('DYLD_LIBRARY_PATH');
            putenv(self::RELAUNCHED);

            return;
        }

        $needed = self::needed(
            os: PHP_OS_FAMILY,
            sqlServer: extension_loaded('pdo_sqlsrv'),
            driverRegistered: is_readable(self::DRIVERS) && str_contains((string) file_get_contents(self::DRIVERS), self::MICROSOFT_DRIVER),
            openSsl3: is_file(self::OPENSSL_3.'/libssl.3.dylib'),
            defaultOpenSsl: realpath(self::DEFAULT_OPENSSL) ?: null,
            canRelaunch: function_exists('pcntl_exec'),
        );

        if (! $needed) {
            return;
        }

        $command = Executable::running()->command();

        pcntl_exec(array_shift($command), [...$command, ...array_slice($_SERVER['argv'], 1)], [
            ...getenv(),
            'DYLD_LIBRARY_PATH' => self::OPENSSL_3,
            self::RELAUNCHED => '1',
        ]);
    }

    public static function needed(
        string $os,
        bool $sqlServer,
        bool $driverRegistered,
        bool $openSsl3,
        ?string $defaultOpenSsl,
        bool $canRelaunch,
    ): bool {
        return $os === 'Darwin'
            && $sqlServer
            && $driverRegistered
            && $openSsl3
            && ! str_contains((string) $defaultOpenSsl, '/openssl@3/')
            && $canRelaunch;
    }
}
