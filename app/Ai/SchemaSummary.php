<?php

namespace App\Ai;

use App\Database\QueryRunner;
use App\Models\Connection;

/**
 * A compact description of the database for the model to work from.
 *
 * Only names and types: no row data ever leaves the machine, so asking a
 * question about a production table does not send its contents anywhere.
 */
class SchemaSummary
{
    public function __construct(private QueryRunner $runner) {}

    public function for(Connection $connection, ?string $focus = null, int $limit = 40): string
    {
        $tables = $this->runner->tables($connection);

        // The table you are looking at goes first, so it survives the limit.
        if ($focus !== null && in_array($focus, $tables, true)) {
            $tables = array_merge([$focus], array_values(array_diff($tables, [$focus])));
        }

        $lines = [];

        foreach (array_slice($tables, 0, $limit) as $table) {
            $lines[] = $table.' ('.$this->columns($connection, $table).')';
        }

        if (count($tables) > $limit) {
            $lines[] = '… and '.(count($tables) - $limit).' more tables';
        }

        return implode("\n", $lines);
    }

    private function columns(Connection $connection, string $table): string
    {
        $columns = [];

        foreach ($this->runner->columns($connection, $table) as $column) {
            $column = (array) $column;
            $name = $column['name'] ?? null;

            if ($name === null) {
                continue;
            }

            $columns[] = $name.' '.($column['type_name'] ?? $column['type'] ?? '?');
        }

        return implode(', ', $columns);
    }
}
