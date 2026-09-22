<?php

namespace App\Mcp\Tools\Concerns;

use App\Models\Connection;
use Laravel\Mcp\Response;

trait ResolvesConnections
{
    protected function resolve(?string $name): ?Connection
    {
        if ($name === null) {
            return null;
        }

        return Connection::where('name', $name)->first();
    }

    protected function unknownConnection(?string $name): Response
    {
        $known = Connection::orderBy('name')->pluck('name')->implode(', ');

        return Response::error(
            "There is no tql connection named [{$name}]. Known connections: {$known}"
        );
    }
}
