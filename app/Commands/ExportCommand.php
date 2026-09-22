<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Models\Connection;
use LaravelZero\Framework\Commands\Command;
use Throwable;

class ExportCommand extends Command
{
    protected $signature = 'export
        {connection : The tql connection name}
        {table? : The table to export, or every table when omitted}
        {--sql= : Where to write the file, a path or a directory}
        {--limit= : Export at most this many rows per table}
        {--list : List the tables in the connection and stop}';

    protected $description = 'Export a table, or a whole database, to re-importable SQL';

    protected $help = <<<'HELP'
    Writes rows as <fg=cyan>insert</> statements you can replay into another database.
    Data only — no schema, so the target table must already exist.

    <fg=yellow>Examples</>

      <fg=green>tql export prod orders --limit=1000 --sql=./orders.sql</>
          a thousand rows of one table into a named file

      <fg=green>tql export prod --sql=./prod.sql</>
          every table in the database, into one file

      <fg=green>tql export prod orders</>
          auto-named file in ~/.config/tql/exports

      <fg=green>tql export prod --list</>
          just show which tables are there

    <fg=yellow>Notes</>

      --sql takes a file path or a directory.
      --limit applies per table, so it bounds a whole-database export too.
      Rows are read in chunks, so a large table does not go through memory at once.
    HELP;

    public function __construct(
        private ConnectionManager $connections,
        private QueryRunner $runner,
        private SqlExporter $exporter,
    ) {
        parent::__construct();
    }

    public function handle(): int
    {
        $connection = Connection::where('name', $this->argument('connection'))->first();

        if ($connection === null) {
            $this->error("No connection named [{$this->argument('connection')}].");
            $this->line('  known: '.Connection::orderBy('name')->pluck('name')->implode(', '));

            return self::FAILURE;
        }

        if ($failure = $this->connections->test($connection)) {
            $this->error($failure);

            return self::FAILURE;
        }

        try {
            $available = $this->runner->tables($connection);
        } catch (Throwable $e) {
            $this->error('Could not read the schema: '.$e->getMessage());

            return self::FAILURE;
        }

        if ($this->option('list')) {
            foreach ($available as $table) {
                $this->line('  '.$table);
            }

            return self::SUCCESS;
        }

        $table = $this->argument('table');

        if ($table !== null && ! in_array($table, $available, true)) {
            $this->error("No table named [{$table}] in [{$connection->name}].");
            $this->line('  known: '.implode(', ', $available));

            return self::FAILURE;
        }

        $tables = $table === null ? $available : [$table];

        if ($tables === []) {
            $this->error('That connection has no tables.');

            return self::FAILURE;
        }

        $limit = $this->option('limit') === null ? null : max(1, (int) $this->option('limit'));

        try {
            $result = $this->exporter->tables($connection, $tables, $this->destination($connection, $tables), $limit);
        } catch (Throwable $e) {
            $this->error('Export failed: '.$e->getMessage());

            return self::FAILURE;
        }

        $this->line(sprintf(
            '  <fg=green>%d rows</> from <fg=cyan>%s</> (%s) in %dms',
            $result->rows,
            count($tables) === 1 ? $tables[0] : count($tables).' tables',
            $result->size(),
            $result->durationMs,
        ));

        $this->line('  '.$result->path);

        return self::SUCCESS;
    }

    private function destination(Connection $connection, array $tables): ?string
    {
        $sql = $this->option('sql');

        if ($sql === null) {
            return null;
        }

        if (is_dir($sql)) {
            $label = count($tables) === 1 ? $tables[0] : 'all';

            return rtrim($sql, '/').'/'.basename($this->exporter->filename($connection, $label));
        }

        return $sql;
    }
}
