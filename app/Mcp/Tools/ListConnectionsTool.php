<?php

namespace App\Mcp\Tools;

use App\Database\AgentAccess;
use App\Mcp\Tools\Concerns\RespondsWithJson;
use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Mcp\Request;
use Laravel\Mcp\Response;
use Laravel\Mcp\Server\Attributes\Description;
use Laravel\Mcp\Server\Tool;

#[Description('List every database connection tql knows about. Use the returned name with the other tql tools.')]
class ListConnectionsTool extends Tool
{
    use RespondsWithJson;

    public function __construct(private AgentAccess $access) {}

    public function handle(Request $request): Response
    {
        $connections = $this->access->connections();

        if ($connections === []) {
            return Response::text('No connections are configured in tql yet.');
        }

        return self::json($connections);
    }

    public function schema(JsonSchema $schema): array
    {
        return [];
    }
}
