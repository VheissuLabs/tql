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

    /**
     * Foreign keys keyed by the local column, for following a link.
     *
     * Only single-column keys: a composite key has no single value to follow
     * from the cell you are standing on.
     *
     * @return array<string, array{table: string, column: string}>
     */
    public function foreignKeys(Connection $connection, string $table): array
    {
        try {
            $keys = $this->connections->resolve($connection)->getSchemaBuilder()->getForeignKeys($table);
        } catch (Throwable) {
            return [];
        }

        $links = [];

        foreach ($keys as $key) {
            $key = (array) $key;

            $columns = $key['columns'] ?? [];
            $foreign = $key['foreign_columns'] ?? [];

            if (count($columns) !== 1 || count($foreign) !== 1) {
                continue;
            }

            $links[$columns[0]] = [
                'table' => (string) ($key['foreign_table'] ?? ''),
                'column' => (string) $foreign[0],
            ];
        }

        return $links;
    }

    /**
     * Tables whose foreign keys point at this one, so a row can be followed
     * backwards: from an artist to their albums.
     *
     * @return array<int, array{table: string, column: string, references: string}>
     */
    public function referencedBy(Connection $connection, string $table): array
    {
        $found = [];

        foreach ($this->tables($connection) as $other) {
            if ($other === $table) {
                continue;
            }

            foreach ($this->foreignKeys($connection, $other) as $column => $link) {
                if ($link['table'] === $table) {
                    $found[] = [
                        'table' => $other,
                        'column' => $column,
                        'references' => $link['column'],
                    ];
                }
            }
        }

        return $found;
    }

    /**
     * Rows of $table where $column equals $value, for loading a relation.
     *
     * @return array<int, array<string, mixed>>
     */
    public function related(Connection $connection, string $table, string $column, mixed $value, int $limit): array
    {
        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();

        $statement = 'select * from '.$grammar->wrapTable($table).
            ' where '.$grammar->wrap($column).' = ? limit '.($limit + 1);

        try {
            return array_map(fn ($row) => (array) $row, $db->select($statement, [$value]));
        } catch (Throwable) {
            return [];
        }
    }

    public function indexes(Connection $connection, string $table): array
    {
        try {
            return array_map(
                fn ($index) => (array) $index,
                $this->connections->resolve($connection)->getSchemaBuilder()->getIndexes($table),
            );
        } catch (Throwable) {
            return [];
        }
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

    /**
     * The statement as the SQL pane should show it: placeholders filled in, so
     * what you read is what ran. It is never sent to the database this way.
     *
     * @param  array<int, mixed>  $bindings
     */
    private function readable(string $statement, array $bindings): string
    {
        foreach ($bindings as $binding) {
            $value = is_numeric($binding)
                ? (string) $binding
                : "'".str_replace("'", "''", (string) $binding)."'";

            $statement = preg_replace('/\?/', $value, $statement, 1) ?? $statement;
        }

        return $statement;
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
        ?Filters $filters = null,
    ): QueryResult {
        $grammar = $this->connections->resolve($connection)->getQueryGrammar();
        $wrapped = $grammar->wrapTable($table);

        $order = $sort === null
            ? ''
            : ' order by '.$grammar->wrap($sort).' '.($direction === 'desc' ? 'desc' : 'asc');

        $built = $filters?->toSql($grammar);
        $where = $built === null ? '' : ' where '.$built[0];

        return $this->run(
            $connection,
            "select * from {$wrapped}{$where}{$order} limit {$limit} offset {$offset}",
            'tui',
            $built[1] ?? [],
        );
    }

    public function grammarFor(Connection $connection): Grammar
    {
        return $this->connections->resolve($connection)->getQueryGrammar();
    }

    /**
     * Delete rows by primary key, all or nothing.
     *
     * @param  array<int, mixed>  $keyValues
     */
    public function delete(Connection $connection, string $table, string $key, array $keyValues): QueryResult
    {
        $started = microtime(true);

        if ($keyValues === []) {
            return new QueryResult(rows: [], durationMs: 0, affected: 0);
        }

        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();

        $statement = 'delete from '.$grammar->wrapTable($table).
            ' where '.$grammar->wrap($key).' in ('.implode(', ', array_fill(0, count($keyValues), '?')).')';

        try {
            $affected = $db->transaction(fn () => $db->delete($statement, array_values($keyValues)));
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, 'tui', true, null, $affected, $duration);

            return new QueryResult(rows: [], durationMs: $duration, affected: $affected, statement: $statement);
        } catch (Throwable $e) {
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, 'tui', false, $e->getMessage(), null, $duration);

            return new QueryResult(rows: [], durationMs: $duration, error: $e->getMessage());
        }
    }

    /**
     * @param  array<int, mixed>  $bindings
     */
    public function run(Connection $connection, string $statement, string $source, array $bindings = []): QueryResult
    {
        $started = microtime(true);

        try {
            $rows = $this->connections->resolve($connection)->select($statement, $bindings);
            $duration = (int) ((microtime(true) - $started) * 1000);

            $this->record($connection, $statement, $source, true, null, count($rows), $duration);

            return new QueryResult(
                rows: array_map(fn ($row) => (array) $row, $rows),
                durationMs: $duration,
                statement: $this->readable($statement, $bindings),
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
        // A connection opened with --peek was never saved, so there is no id
        // to hang history off. History is a convenience; never let it be the
        // thing that fails a query the user actually asked for.
        if ($connection->id === null) {
            return;
        }

        try {
            QueryExecution::create([
                'connection_id' => $connection->id,
                'statement' => $statement,
                'source' => $source,
                'succeeded' => $succeeded,
                'error' => $error,
                'row_count' => $rowCount,
                'duration_ms' => $durationMs,
            ]);
        } catch (Throwable) {
            // Not worth surfacing: the query itself already succeeded or
            // failed on its own terms.
        }
    }
}
