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

#[Description('Describe the columns of a table in a tql connection, including types and nullability.')]
class DescribeTableTool extends Tool
{
    use RespondsWithJson;

    public function __construct(private AgentAccess $access) {}

    public function handle(Request $request): Response
    {
        try {
            return self::json($this->access->describe($request->get('connection'), (string) $request->get('table')));
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

            'table' => $schema->string()
                ->description('The table to describe.')
                ->required(),
        ];
    }
}
