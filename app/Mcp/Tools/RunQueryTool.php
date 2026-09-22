<?php

namespace App\Mcp\Tools;

use App\Database\QueryRunner;
use App\Mcp\Tools\Concerns\ResolvesConnections;
use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Mcp\Request;
use Laravel\Mcp\Response;
use Laravel\Mcp\Server\Attributes\Description;
use Laravel\Mcp\Server\Tool;

#[Description('Run a read-only SQL query against a dotsql connection. Only SELECT, SHOW, EXPLAIN, DESCRIBE and PRAGMA statements are permitted.')]
class RunQueryTool extends Tool
{
    use ResolvesConnections;

    public function __construct(private QueryRunner $runner) {}

    public function handle(Request $request): Response
    {
        $connection = $this->resolve($request->get('connection'));

        if ($connection === null) {
            return $this->unknownConnection($request->get('connection'));
        }

        $statement = trim((string) $request->get('query'));

        if (! $this->runner->isReadOnly($statement)) {
            return Response::error(
                'Only read-only statements are permitted through MCP. '.
                'Use the dotsql interface directly to modify data.'
            );
        }

        $result = $this->runner->run($connection, $statement, 'mcp');

        if ($result->failed()) {
            return Response::error($result->error);
        }

        return Response::text(json_encode([
            'connection' => $connection->name,
            'rows' => $result->count(),
            'duration_ms' => $result->durationMs,
            'results' => array_slice($result->rows, 0, 200),
            'truncated' => $result->count() > 200,
        ], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
    }

    public function schema(JsonSchema $schema): array
    {
        return [
            'connection' => $schema->string()
                ->description('The dotsql connection name.')
                ->required(),

            'query' => $schema->string()
                ->description('A read-only SQL statement.')
                ->required(),
        ];
    }
}
