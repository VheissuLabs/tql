<?php

namespace App\Database;

use App\Models\Connection;
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
            return $e->getMessage();
        }
    }

    public function touch(Connection $connection): void
    {
        $connection->forceFill(['last_used_at' => now()])->save();
    }
}
