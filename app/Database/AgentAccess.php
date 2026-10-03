<?php

namespace App\Database;

use App\Models\Connection;
use Throwable;

class AgentAccess
{
    public const LIMIT = 200;

    public function __construct(private QueryRunner $runner) {}

    public function connections(): array
    {
        return Connection::orderBy('name')->get()->map(fn (Connection $connection) => [
            'name' => $connection->name,
            'driver' => $connection->driver,
            'target' => $connection->describe(),
            'read_only' => $connection->read_only,
            'last_used_at' => $connection->last_used_at?->toIso8601String(),
        ])->all();
    }

    public function tables(?string $name, ?string $database = null): array
    {
        $connection = $this->connection($name, $database);

        try {
            $tables = $this->runner->tables($connection);
        } catch (Throwable $failure) {
            throw new AccessRefused('Could not read the schema: '.$failure->getMessage());
        }

        return ['connection' => $connection->name, 'tables' => $tables];
    }

    public function describe(?string $name, string $table, ?string $database = null): array
    {
        $connection = $this->connection($name, $database);

        try {
            $columns = $this->runner->columns($connection, $table);
        } catch (Throwable $failure) {
            throw new AccessRefused("Could not describe [{$table}]: ".$failure->getMessage());
        }

        if ($columns === []) {
            throw new AccessRefused("Table [{$table}] has no columns, or does not exist.");
        }

        return [
            'connection' => $connection->name,
            'table' => $table,
            'primary_key' => $this->runner->primaryKey($connection, $table),
            'columns' => $columns,
        ];
    }

    public function query(?string $name, string $statement, string $source, ?string $database = null, int $limit = self::LIMIT): array
    {
        $connection = $this->connection($name, $database);
        $statement = trim($statement);

        if (! $this->runner->isReadOnly($statement)) {
            throw new AccessRefused('Only read-only statements are allowed here: select, show, explain, describe, pragma or with, one at a time. Change data in the tql interface.');
        }

        $result = $this->runner->run($connection, $statement, $source);

        if ($result->failed()) {
            throw new AccessRefused((string) $result->error);
        }

        return [
            'connection' => $connection->name,
            'rows' => $result->count(),
            'duration_ms' => $result->durationMs,
            'results' => array_slice($result->rows, 0, max(1, $limit)),
            'truncated' => $result->count() > max(1, $limit),
        ];
    }

    private function connection(?string $name, ?string $database): Connection
    {
        $connection = $name === null
            ? null
            : Connection::where('name', $name)->first();

        if ($connection === null) {
            $known = Connection::orderBy('name')->pluck('name')->implode(', ');

            throw new AccessRefused("There is no tql connection named [{$name}]. Known connections: {$known}");
        }

        if ($database !== null) {
            $connection->sessionDatabase = $database;
        }

        return $connection;
    }
}
