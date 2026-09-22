<?php

namespace App\Database;

use App\Models\Connection;
use App\Models\QueryExecution;
use Illuminate\Database\Query\Grammars\Grammar;
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

    public function primaryKey(Connection $connection, string $table): ?string
    {
        $indexes = $this->connections->resolve($connection)->getSchemaBuilder()->getIndexes($table);

        foreach ($indexes as $index) {
            $index = (array) $index;

            if (($index['primary'] ?? false) && count($index['columns'] ?? []) === 1) {
                return $index['columns'][0];
            }
        }

        return null;
    }

    public function isReadOnly(string $statement): bool
    {
        $normalised = ltrim($statement);
        $normalised = preg_replace('/^(--[^\n]*\n|\/\*.*?\*\/|\s)+/s', '', $normalised) ?? $normalised;

        if (str_contains(rtrim($normalised, "; \n\r\t"), ';')) {
            return false;
        }

        return (bool) preg_match('/^(select|show|explain|describe|desc|pragma|with)\b/i', $normalised);
    }

    public function update(Connection $connection, string $table, string $column, string $key, mixed $keyValue, mixed $value): QueryResult
    {
        $started = microtime(true);

        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();

        $statement = 'update '.$grammar->wrapTable($table).
            ' set '.$grammar->wrap($column).' = ?'.
            ' where '.$grammar->wrap($key).' = ?';

        try {
            $affected = $db->update($statement, [$value, $keyValue]);
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, 'tui', true, null, $affected, $duration);

            return new QueryResult(rows: [], durationMs: $duration, affected: $affected);
        } catch (Throwable $e) {
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, 'tui', false, $e->getMessage(), null, $duration);

            return new QueryResult(rows: [], durationMs: $duration, error: $e->getMessage());
        }
    }

    public function rows(
        Connection $connection,
        string $table,
        int $limit = 50,
        int $offset = 0,
        ?string $sort = null,
        string $direction = 'asc',
    ): QueryResult {
        $grammar = $this->connections->resolve($connection)->getQueryGrammar();
        $wrapped = $grammar->wrapTable($table);

        $order = $sort === null
            ? ''
            : ' order by '.$grammar->wrap($sort).' '.($direction === 'desc' ? 'desc' : 'asc');

        return $this->run($connection, "select * from {$wrapped}{$order} limit {$limit} offset {$offset}", 'tui');
    }

    public function grammarFor(Connection $connection): Grammar
    {
        return $this->connections->resolve($connection)->getQueryGrammar();
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
                statement: $statement,
            );
        } catch (Throwable $e) {
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, $source, false, $e->getMessage(), null, $duration);

            return new QueryResult(rows: [], durationMs: $duration, error: $e->getMessage(), statement: $statement);
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
