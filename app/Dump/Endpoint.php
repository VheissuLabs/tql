<?php

namespace App\Dump;

use App\Models\Connection;
use App\Ssh\Tunnel;

class Endpoint
{
    public function __construct(public string $host, public int $port) {}

    public static function of(Connection $connection): self
    {
        if ($connection->usesSsh()) {
            return new self('127.0.0.1', Tunnel::for($connection)->port);
        }

        return new self((string) $connection->host, (int) $connection->port);
    }
}
