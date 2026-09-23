<?php

namespace App\Database;

use App\Models\Connection;
use App\Models\QueryExecution;
use Illuminate\Database\Query\Grammars\Grammar;
use Throwable;

class QueryRunner
{
    /**
     * Schema answers, kept for as long as the table on screen is.
     *
     * The interface asks what a column links to on every frame, and the row
     * inspector asks what points back at a table before it can draw. Those are
     * schema queries, and a schema does not change between key presses.
     *
     * @var array<string, mixed>
     */
    private array $schema = [];

    public function __construct(private ConnectionManager $connections) {}

    /**
     * Forget it, for when something might have changed it: a reload, a write,
     * or a statement the user wrote themselves.
     */
    public function forgetSchema(): void
    {
        $this->schema = [];
    }

    /**
     * @template T
     *
     * @param  callable(): T  $answer
     * @return T
     */
    private function remembered(string $key, callable $answer): mixed
    {
        return $this->schema[$key] ??= $answer();
    }

    public function tables(Connection $connection): array
    {
        return array_map(
            fn ($table) => is_array($table) ? $table['name'] : $table->name,
            $this->connections->resolve($connection)->getSchemaBuilder()->getTables($this->schema($connection))
        );
    }

    /**
     * MySQL asks for tables across every schema on the server unless it is
     * told which one, so the database is the schema there. Postgres and SQL
     * Server already answer within the database they are connected to.
     */
    private function schema(Connection $connection): ?string
    {
        if (! in_array($connection->driver, ['mysql', 'mariadb'], true)) {
            return null;
        }

        return trim((string) $connection->activeDatabase()) ?: null;
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
        return $this->remembered('keys.'.$connection->id.'.'.$table, fn () => $this->findForeignKeys($connection, $table));
    }

    /**
     * @return array<string, array{table: string, column: string}>
     */
    private function findForeignKeys(Connection $connection, string $table): array
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
        return $this->remembered('referenced.'.$connection->id.'.'.$table, fn () => $this->findReferences($connection, $table));
    }

    /**
     * @return array<int, array{table: string, column: string, references: string, unique: bool}>
     */
    private function findReferences(Connection $connection, string $table): array
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
                        // A unique key on the other side means one row, not
                        // many: that is the whole difference between a profile
                        // and a list of orders.
                        'unique' => in_array($column, $this->uniqueColumns($connection, $other), true),
                    ];
                }
            }
        }

        return $found;
    }

    /**
     * Columns that can hold a value only once: a single-column unique index,
     * or a single-column primary key.
     *
     * @return array<int, string>
     */
    public function uniqueColumns(Connection $connection, string $table): array
    {
        $unique = [];

        foreach ($this->indexes($connection, $table) as $index) {
            $columns = (array) ($index['columns'] ?? []);

            if (count($columns) !== 1) {
                continue;
            }

            if (($index['unique'] ?? false) || ($index['primary'] ?? false)) {
                $unique[] = (string) $columns[0];
            }
        }

        return array_values(array_unique($unique));
    }

    /**
     * Rows of $table where $column equals $value, for loading a relation.
     *
     * @return array<int, array<string, mixed>>
     */
    /**
     * Is this table there only to join two others?
     *
     * A pivot has two foreign keys and nothing of its own worth reading — an
     * id and a timestamp at most. Showing its rows shows a list of timestamps;
     * what you wanted was what is on the other side of it.
     *
     * @return array{table: string, on: string, references: string}|null
     */
    public function pivot(Connection $connection, string $table, string $joinedOn): ?array
    {
        $keys = $this->foreignKeys($connection, $table);

        if (count($keys) !== 2 || ! isset($keys[$joinedOn])) {
            return null;
        }

        $far = null;

        foreach ($keys as $column => $link) {
            if ($column !== $joinedOn) {
                $far = ['table' => $link['table'], 'on' => $column, 'references' => $link['column']];
            }
        }

        if ($far === null) {
            return null;
        }

        // Anything else in there is data, and data is worth showing as itself.
        foreach ($this->columns($connection, $table) as $column) {
            $name = (string) ((array) $column)['name'];

            if (isset($keys[$name]) || static::isPlumbing($name)) {
                continue;
            }

            return null;
        }

        return $far;
    }

    /**
     * Columns every table has and nobody reads on purpose.
     */
    private static function isPlumbing(string $column): bool
    {
        return in_array(strtolower($column), [
            'id', 'created_at', 'updated_at', 'deleted_at', 'last_update', 'last_updated',
        ], true);
    }

    /**
     * The rows on the far side of a pivot: a film's actors, not its film_actor
     * rows.
     *
     * @param  array{table: string, on: string, references: string}  $far
     * @return array<int, array<string, mixed>>
     */
    public function through(
        Connection $connection,
        string $pivot,
        string $joinedOn,
        array $far,
        mixed $value,
        int $limit,
    ): array {
        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();

        $statement = 'select '.$grammar->wrapTable($far['table']).'.* from '.$grammar->wrapTable($far['table'])
            .' join '.$grammar->wrapTable($pivot)
            .' on '.$grammar->wrapTable($pivot).'.'.$grammar->wrap($far['on'])
            .' = '.$grammar->wrapTable($far['table']).'.'.$grammar->wrap($far['references'])
            .' where '.$grammar->wrapTable($pivot).'.'.$grammar->wrap($joinedOn).' = ?'
            .' limit '.($limit + 1);

        try {
            return array_map(fn ($row) => (array) $row, $db->select($statement, [$value]));
        } catch (Throwable) {
            return [];
        }
    }

    /**
     * How many are on the far side, when there are more than were asked for.
     *
     * @param  array{table: string, on: string, references: string}  $far
     */
    public function countThrough(Connection $connection, string $pivot, string $joinedOn, array $far, mixed $value): ?int
    {
        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();

        $statement = 'select count(*) as total from '.$grammar->wrapTable($pivot)
            .' where '.$grammar->wrap($joinedOn).' = ?';

        try {
            return (int) ((array) ($db->select($statement, [$value])[0] ?? [])['total'] ?? 0);
        } catch (Throwable) {
            return null;
        }
    }

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

    /**
     * The databases on this server, for a connection that is not pinned to
     * one. sqlite is a single file, so it has exactly one.
     *
     * @return array<int, string>
     */
    public function databases(Connection $connection): array
    {
        if ($connection->driver === 'sqlite') {
            return [];
        }

        $statement = match ($connection->driver) {
            'pgsql' => 'select datname as name from pg_database where datistemplate = false order by datname',
            'sqlsrv' => 'select name from sys.databases order by name',
            default => 'show databases',
        };

        try {
            $rows = $this->connections->resolve($connection)->select($statement);
        } catch (Throwable) {
            return [];
        }

        return array_values(array_filter(array_map(
            fn ($row) => (string) (((array) $row)['name'] ?? ((array) $row)['Database'] ?? ''),
            $rows,
        )));
    }

    public function indexes(Connection $connection, string $table): array
    {
        return $this->remembered('indexes.'.$connection->id.'.'.$table, function () use ($connection, $table) {
            try {
                return array_map(
                    fn ($index) => (array) $index,
                    $this->connections->resolve($connection)->getSchemaBuilder()->getIndexes($table),
                );
            } catch (Throwable) {
                return [];
            }
        });
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
    /**
     * Add a row. Only the columns that were filled in are sent, so a column
     * left alone takes whatever default the table gives it.
     *
     * @param  array<string, mixed>  $values
     */
    public function insert(Connection $connection, string $table, array $values, string $source = 'tui'): QueryResult
    {
        $grammar = $this->grammarFor($connection);

        if ($values === []) {
            // Every column defaulted: still a row, and every driver spells
            // that differently.
            $statement = $connection->driver === 'sqlite'
                ? 'insert into '.$grammar->wrapTable($table).' default values'
                : 'insert into '.$grammar->wrapTable($table).' () values ()';

            return $this->run($connection, $statement, $source);
        }

        $columns = array_map(fn (string $column) => $grammar->wrap($column), array_keys($values));

        $statement = 'insert into '.$grammar->wrapTable($table)
            .' ('.implode(', ', $columns).')'
            .' values ('.implode(', ', array_fill(0, count($values), '?')).')';

        return $this->run($connection, $statement, $source, array_values($values));
    }

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
