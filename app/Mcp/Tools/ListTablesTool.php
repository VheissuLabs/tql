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

#[Description('List the tables in a tql connection.')]
class ListTablesTool extends Tool
{
    use RespondsWithJson;

    public function __construct(private AgentAccess $access) {}

    public function handle(Request $request): Response
    {
        try {
            return self::json($this->access->tables($request->get('connection')));
        } catch (AccessRefused $refused) {
            return Response::error($refused->getMessage());
        }
    }

    public function schema(JsonSchema $schema): array
    {
        return [
            'connection' => $schema->string()
                ->description('The tql connection name, as returned by the list connections tool.')
                ->required(),
        ];
    }
}
