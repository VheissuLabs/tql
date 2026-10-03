<?php

namespace App\Mcp\Tools\Concerns;

use Laravel\Mcp\Response;

trait RespondsWithJson
{
    protected static function json(array $data): Response
    {
        return Response::text(json_encode($data, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
    }
}
