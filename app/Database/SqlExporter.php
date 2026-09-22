<?php

namespace App\Database;

use App\Models\Connection;
use App\Models\Setting;
use App\Support\Paths;
use PDO;
use RuntimeException;

class SqlExporter
{
    private const CHUNK = 500;

    private const PER_STATEMENT = 100;

    public function __construct(private ConnectionManager $connections) {}

    public const REMEMBERED = 'export_directory';

    /**
     * Where an unnamed export goes: what the config says, else wherever the
     * last one was saved, else tql's own folder. Taking the trouble to save
     * something in ~/Downloads is worth remembering.
     */
    public function directory(): string
    {
        $remembered = Setting::read(self::REMEMBERED);

        $path = (string) (
            config('tql.ui.export_path')
            ?: ($remembered !== null && is_dir($remembered) ? $remembered : null)
            ?: Paths::configDirectory().'/exports'
        );

        if (! is_dir($path)) {
            mkdir($path, 0700, true);
        }

        return $path;
    }

    /**
     * Keep the folder an export was written to, whether tql chose it or the
     * user typed it, so the next one is offered in the same place.
     */
    private function remember(string $path): void
    {
        $directory = dirname($path);

        if ($directory !== Setting::read(self::REMEMBERED)) {
            Setting::write(self::REMEMBERED, $directory);
        }
    }

    /**
     * Where an unnamed export lands: the database, what was taken out of it,
     * and when. A whole-database export leaves the middle part off, since the
     * database is the answer to what is in it.
     */
    public function filename(Connection $connection, ?string $table = null): string
    {
        $slug = fn (string $v) => strtolower(preg_replace('/[^A-Za-z0-9]+/', '-', $v) ?: 'export');

        // Named for the database rather than the connection: the file is going
        // to be read back into a database, and "mysql-dev" says nothing about
        // what is in it.
        $subject = $connection->driver === 'sqlite'
            ? pathinfo((string) $connection->database, PATHINFO_FILENAME)
            : (string) $connection->activeDatabase();

        $subject = $slug($subject ?: $connection->name);

        $part = $table === null ? '' : $slug($table).'-';

        return $this->directory().'/'.$subject.'-'.$part.date('Ymd-His').'.sql';
    }

    public function tables(Connection $connection, array $tables, ?string $path = null, ?int $limit = null): ExportResult
    {
        $path ??= $this->filename($connection, count($tables) === 1 ? $tables[0] : null);

        $started = microtime(true);
        $rows = 0;

        $handle = fopen($path, 'w');

        if ($handle === false) {
            throw new RuntimeException("Could not write to [{$path}].");
        }

        fwrite($handle, $this->header($connection, implode(', ', $tables)));
        fclose($handle);

        foreach ($tables as $table) {
            $rows += $this->appendTable($connection, $table, $path, $limit);
        }

        $this->remember($path);

        return new ExportResult(
            path: $path,
            rows: $rows,
            bytes: filesize($path) ?: 0,
            durationMs: (int) ((microtime(true) - $started) * 1000),
        );
    }

    public function table(Connection $connection, string $table, ?string $path = null, ?int $limit = null): ExportResult
    {
        $path ??= $this->filename($connection, $table);

        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();
        $pdo = $db->getPdo();

        $handle = fopen($path, 'w');

        if ($handle === false) {
            throw new RuntimeException("Could not write to [{$path}].");
        }

        $started = microtime(true);

        fwrite($handle, $this->header($connection, $table));

        $columns = array_map(
            fn ($column) => is_array($column) ? $column['name'] : $column->name,
            $db->getSchemaBuilder()->getColumns($table)
        );

        $wrappedTable = $grammar->wrapTable($table);
        $wrappedColumns = implode(', ', array_map(fn ($c) => $grammar->wrap($c), $columns));

        $key = (new QueryRunner($this->connections))->primaryKey($connection, $table);

        $written = 0;
        $offset = 0;
        $buffer = [];

        while (true) {
            $take = $limit === null ? self::CHUNK : min(self::CHUNK, $limit - $written);

            if ($take <= 0) {
                break;
            }

            $order = $key === null ? '' : ' order by '.$grammar->wrap($key);
            $sql = "select * from {$wrappedTable}{$order} limit {$take} offset {$offset}";

            $rows = $db->select($sql);

            if ($rows === []) {
                break;
            }

            foreach ($rows as $row) {
                $buffer[] = '('.implode(', ', array_map(
                    fn ($value) => $this->literal($pdo, $value),
                    array_values((array) $row)
                )).')';

                $written++;

                if (count($buffer) >= self::PER_STATEMENT) {
                    $this->flush($handle, $wrappedTable, $wrappedColumns, $buffer);
                }
            }

            $offset += count($rows);
        }

        $this->flush($handle, $wrappedTable, $wrappedColumns, $buffer);

        fclose($handle);

        $this->remember($path);

        return new ExportResult(
            path: $path,
            rows: $written,
            bytes: filesize($path) ?: 0,
            durationMs: (int) ((microtime(true) - $started) * 1000),
        );
    }

    public function rows(Connection $connection, string $label, array $columns, array $rows, ?string $path = null): ExportResult
    {
        $path ??= $this->filename($connection, $label);

        $db = $this->connections->resolve($connection);
        $grammar = $db->getQueryGrammar();
        $pdo = $db->getPdo();

        $handle = fopen($path, 'w');

        if ($handle === false) {
            throw new RuntimeException("Could not write to [{$path}].");
        }

        $started = microtime(true);

        fwrite($handle, $this->header($connection, $label));

        $wrappedTable = $grammar->wrapTable($label);
        $wrappedColumns = implode(', ', array_map(fn ($c) => $grammar->wrap($c), $columns));

        $buffer = [];

        foreach ($rows as $row) {
            $buffer[] = '('.implode(', ', array_map(
                fn ($value) => $this->literal($pdo, $value),
                array_values((array) $row)
            )).')';

            if (count($buffer) >= self::PER_STATEMENT) {
                $this->flush($handle, $wrappedTable, $wrappedColumns, $buffer);
            }
        }

        $this->flush($handle, $wrappedTable, $wrappedColumns, $buffer);

        fclose($handle);

        $this->remember($path);

        return new ExportResult(
            path: $path,
            rows: count($rows),
            bytes: filesize($path) ?: 0,
            durationMs: (int) ((microtime(true) - $started) * 1000),
        );
    }

    private function appendTable(Connection $connection, string $table, string $path, ?int $limit): int
    {
        $temporary = $path.'.part';

        $result = $this->table($connection, $table, $temporary, $limit);

        $body = file_get_contents($temporary) ?: '';
        $body = preg_replace('/\A(--[^\n]*\n)+\n?/', '', $body) ?? $body;

        file_put_contents($path, "-- {$table}\n".$body, FILE_APPEND);

        unlink($temporary);

        return $result->rows;
    }

    private function flush($handle, string $table, string $columns, array &$buffer): void
    {
        if ($buffer === []) {
            return;
        }

        fwrite($handle, "insert into {$table} ({$columns}) values\n".implode(",\n", $buffer).";\n\n");

        $buffer = [];
    }

    private function header(Connection $connection, string $table): string
    {
        return "-- tql export\n".
            "-- connection: {$connection->name} ({$connection->driver})\n".
            "-- table: {$table}\n".
            '-- exported: '.date('c')."\n\n";
    }

    private function literal(PDO $pdo, mixed $value): string
    {
        return match (true) {
            $value === null => 'null',
            is_bool($value) => $value ? '1' : '0',
            is_int($value), is_float($value) => (string) $value,
            default => $pdo->quote((string) $value),
        };
    }
}
