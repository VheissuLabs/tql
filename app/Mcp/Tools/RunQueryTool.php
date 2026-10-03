<?php

namespace App\Mcp\Tools;

use App\Database\AccessRefused;
use App\Database\AgentAccess;
use App\Mcp\Tools\Concerns\RespondsWithJson;
use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Mcp\Request;
use Laravel\Mcp\Response;
use Laravel\Mcp\Server\Attributes\Description;
use Laravel\Mcp\Server\Tool;

#[Description('Run a read-only SQL query against a tql connection. Only SELECT, SHOW, EXPLAIN, DESCRIBE and PRAGMA statements are permitted.')]
class RunQueryTool extends Tool
{
    use RespondsWithJson;

    public function __construct(private AgentAccess $access) {}

    public function handle(Request $request): Response
    {
        try {
            return self::json($this->access->query($request->get('connection'), (string) $request->get('query'), 'mcp'));
        } catch (AccessRefused $refused) {
            return Response::error($refused->getMessage());
        }
    }

    public function schema(JsonSchema $schema): array
    {
        return [
            'connection' => $schema->string()
                ->description('The tql connection name.')
                ->required(),

            'query' => $schema->string()
                ->description('A read-only SQL statement.')
                ->required(),
        ];
    }
}
