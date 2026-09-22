<?php

namespace App\Mcp\Tools;

use App\Database\QueryRunner;
use App\Mcp\Tools\Concerns\ResolvesConnections;
use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Mcp\Request;
use Laravel\Mcp\Response;
use Laravel\Mcp\Server\Attributes\Description;
use Laravel\Mcp\Server\Tool;
use Throwable;

#[Description('List the tables in a tql connection.')]
class ListTablesTool extends Tool
{
    use ResolvesConnections;

    public function __construct(private QueryRunner $runner) {}

    public function handle(Request $request): Response
    {
        $connection = $this->resolve($request->get('connection'));

        if ($connection === null) {
            return $this->unknownConnection($request->get('connection'));
        }

        try {
            $tables = $this->runner->tables($connection);
        } catch (Throwable $e) {
            return Response::error('Could not read the schema: '.$e->getMessage());
        }

        return Response::text(json_encode([
            'connection' => $connection->name,
            'tables' => $tables,
        ], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
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
