<?php

namespace App\Database;

use App\Models\Connection;
use App\Models\QueryExecution;
use Throwable;

class QueryRunner
{
    public function __construct(private ConnectionManager $connections) {}

    public function tables(Connection $connection): array
    {
        return array_map(
            fn ($table) => is_array($table) ? $table['name'] : $table->name,
            $this->connections->resolve($connection)->getSchemaBuilder()->getTables()
        );
    }

    public function columns(Connection $connection, string $table): array
    {
        return $this->connections->resolve($connection)->getSchemaBuilder()->getColumns($table);
    }

    public function rows(Connection $connection, string $table, int $limit = 50, int $offset = 0): QueryResult
    {
        $grammar = $this->connections->resolve($connection)->getQueryGrammar();
        $wrapped = $grammar->wrapTable($table);

        return $this->run($connection, "select * from {$wrapped} limit {$limit} offset {$offset}", 'tui');
    }

    public function run(Connection $connection, string $statement, string $source): QueryResult
    {
        $started = microtime(true);

        try {
            $rows = $this->connections->resolve($connection)->select($statement);
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, $source, true, null, count($rows), $duration);

            return new QueryResult(
                rows: array_map(fn ($row) => (array) $row, $rows),
                durationMs: $duration,
            );
        } catch (Throwable $e) {
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, $source, false, $e->getMessage(), null, $duration);

            return new QueryResult(rows: [], durationMs: $duration, error: $e->getMessage());
        }
    }

    private function record(
        Connection $connection,
        string $statement,
        string $source,
        bool $succeeded,
        ?string $error,
        ?int $rowCount,
        int $durationMs,
    ): void {
        QueryExecution::create([
            'connection_id' => $connection->id,
            'statement' => $statement,
            'source' => $source,
            'succeeded' => $succeeded,
            'error' => $error,
            'row_count' => $rowCount,
            'duration_ms' => $durationMs,
        ]);
    }
}
