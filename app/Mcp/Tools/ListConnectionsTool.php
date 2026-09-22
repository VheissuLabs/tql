<?php

namespace App\Mcp\Tools;

use App\Models\Connection;
use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Mcp\Request;
use Laravel\Mcp\Response;
use Laravel\Mcp\Server\Attributes\Description;
use Laravel\Mcp\Server\Tool;

#[Description('List every database connection tql knows about. Use the returned name with the other tql tools.')]
class ListConnectionsTool extends Tool
{
    public function handle(Request $request): Response
    {
        $connections = Connection::orderBy('name')->get()->map(fn (Connection $c) => [
            'name' => $c->name,
            'driver' => $c->driver,
            'target' => $c->describe(),
            'read_only' => $c->read_only,
            'last_used_at' => $c->last_used_at?->toIso8601String(),
        ]);

        if ($connections->isEmpty()) {
            return Response::text('No connections are configured in tql yet.');
        }

        return Response::text(json_encode($connections, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
    }

    public function schema(JsonSchema $schema): array
    {
        return [];
    }
}
