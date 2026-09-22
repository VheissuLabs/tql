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

#[Description('Describe the columns of a table in a tql connection, including types and nullability.')]
class DescribeTableTool extends Tool
{
    use ResolvesConnections;

    public function __construct(private QueryRunner $runner) {}

    public function handle(Request $request): Response
    {
        $connection = $this->resolve($request->get('connection'));

        if ($connection === null) {
            return $this->unknownConnection($request->get('connection'));
        }

        $table = (string) $request->get('table');

        try {
            $columns = $this->runner->columns($connection, $table);
        } catch (Throwable $e) {
            return Response::error("Could not describe [{$table}]: ".$e->getMessage());
        }

        if ($columns === []) {
            return Response::error("Table [{$table}] has no columns, or does not exist.");
        }

        return Response::text(json_encode([
            'connection' => $connection->name,
            'table' => $table,
            'primary_key' => $this->runner->primaryKey($connection, $table),
            'columns' => $columns,
        ], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
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
