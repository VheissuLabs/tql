<?php

namespace App\Database;

use App\Models\Connection;
use App\Ssh\Tunnel;
use Illuminate\Database\Connection as IlluminateConnection;
use Illuminate\Support\Facades\Config;
use Illuminate\Support\Facades\DB;
use PDO;
use RuntimeException;

class ConnectionManager
{
    public function available(string $driver): bool
    {
        return in_array($driver, PDO::getAvailableDrivers(), true);
    }

    public function drivers(): array
    {
        return PDO::getAvailableDrivers();
    }

    public function resolve(Connection $connection): IlluminateConnection
    {
        if (! $this->available($connection->driver)) {
            throw new RuntimeException(
                "The [{$connection->driver}] PDO driver is not available in this PHP build."
            );
        }

        // An unsaved connection (tql open <file>) has no id yet.
        $handle = 'tql_target_'.($connection->id ?? substr(md5((string) $connection->database), 0, 12));

        $config = $connection->toLaravelConfig();

        // An ssh connection reaches the database through a local port that
        // the tunnel forwards, so the driver still only ever sees localhost.
        if ($connection->usesSsh()) {
            $tunnel = Tunnel::for($connection);

            $config['host'] = '127.0.0.1';
            $config['port'] = $tunnel->port;
        }

        if (Config::get("database.connections.{$handle}") === $config) {
            return DB::connection($handle);
        }

        Config::set("database.connections.{$handle}", $config);

        DB::purge($handle);

        return DB::connection($handle);
    }

    public function test(Connection $connection): ?string
    {
        try {
            $this->resolve($connection)->getPdo();

            return null;
        } catch (\Throwable $e) {
            return static::explain($e->getMessage());
        }
    }

    public static function explain(string $message): string
    {
        if (str_contains($message, 'OpenSSL library could not be loaded')) {
            return $message.' Microsoft\'s driver needs OpenSSL 3, and Homebrew now defaults to OpenSSL 4. Run: brew install openssl@3, then start tql again.';
        }

        if (! str_contains($message, 'Microsoft ODBC Driver')) {
            return $message;
        }

        return $message.(PHP_OS_FAMILY === 'Darwin'
            ? ' Install it once with: brew tap microsoft/mssql-release && HOMEBREW_ACCEPT_EULA=Y brew install msodbcsql18 openssl@3 (newer Homebrew first asks you to run: brew trust microsoft/mssql-release)'
            : ' Install it once: https://learn.microsoft.com/sql/connect/odbc/linux-mac/installing-the-microsoft-odbc-driver-for-sql-server');
    }

    public function touch(Connection $connection): void
    {
        $connection->forceFill(['last_used_at' => now()])->save();
    }
}
