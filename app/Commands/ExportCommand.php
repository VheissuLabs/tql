<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Models\Connection;
use App\Support\Paths;
use LaravelZero\Framework\Commands\Command;
use Throwable;

use function Laravel\Prompts\search;
use function Laravel\Prompts\select;
use function Laravel\Prompts\text;

class ExportCommand extends Command
{
    protected $signature = 'export
        {connection? : The tql connection name, asked for when left out}
        {table? : The table to export, or every table when omitted}
        {--sql= : Where to write the file, a path or a directory}
        {--limit= : Export at most this many rows per table}
        {--list : List the tables in the connection and stop}';

    protected $description = 'Export a table, or a whole database, to re-importable SQL';

    protected $help = <<<'HELP'
    Writes rows as <fg=cyan>insert</> statements you can replay into another database.
    Data only — no schema, so the target table must already exist.

    <fg=yellow>Examples</>

      <fg=green>tql export</>
          asks which connection and which table

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

    /**
     * Whether the connection was chosen from a list rather than typed.
     */
    private bool $asked = false;

    public function handle(): int
    {
        $connection = $this->connection();

        if ($connection === null) {
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

        // Nothing was typed at all, so keep asking rather than assuming the
        // whole database is what they meant.
        if ($table === null && $this->asked) {
            $table = $this->chooseTable($available);
        }

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

    /**
     * The connection to export from: the one named, or one picked off the list.
     */
    private function connection(): ?Connection
    {
        $name = $this->argument('connection');

        if ($name !== null) {
            $connection = Connection::where('name', $name)->first();

            if ($connection === null) {
                $this->error("No connection named [{$name}].");
                $this->line('  known: '.Connection::orderBy('name')->pluck('name')->implode(', '));
            }

            return $connection;
        }

        $connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();

        if ($connections->isEmpty()) {
            $this->error('No saved connections.');
            $this->line('  <fg=green>tql open <path-or-dsn></> saves one.');

            return null;
        }

        if (! $this->input->isInteractive()) {
            $this->error('Which connection? Name one, or run it in a terminal to be asked.');
            $this->line('  known: '.$connections->pluck('name')->implode(', '));

            return null;
        }

        $this->asked = true;

        $chosen = select(
            label: 'Export from',
            options: $connections->mapWithKeys(fn (Connection $c) => [
                $c->id => $c->name.'  ·  '.$c->describe(),
            ])->all(),
            scroll: 10,
        );

        return $connections->firstWhere('id', $chosen);
    }

    /**
     * Which table, with the whole database as the first answer.
     *
     * @param  string[]  $available
     */
    /**
     * Where to write it, offering the auto-named file as the answer.
     *
     * @param  string[]  $tables
     */
    private function askWhere(Connection $connection, array $tables): ?string
    {
        $suggested = $this->exporter->filename(
            $connection, count($tables) === 1 ? $tables[0] : 'all',
        );

        $answer = trim(text(
            label: 'Save it where?',
            default: $suggested,
            hint: 'a file, or a folder to have it named for you',
            validate: fn (string $value) => is_dir($dir = dirname(Paths::expand(trim($value) ?: $suggested)))
                ? null
                : "There is no folder [{$dir}].",
        ));

        return $answer === '' ? null : $answer;
    }

    private function chooseTable(array $available): ?string
    {
        $every = 'every table ('.count($available).')';

        if (count($available) <= 15) {
            $chosen = select(label: 'Which table?', options: ['' => $every, ...array_combine($available, $available)], scroll: 15);

            return $chosen === '' ? null : (string) $chosen;
        }

        $chosen = search(
            label: 'Which table?',
            placeholder: 'type to filter, or pick the first for all of them',
            options: fn (string $typed) => ['' => $every, ...array_combine(
                $matches = array_values(array_filter($available, fn ($table) => $typed === '' || str_contains(strtolower($table), strtolower($typed)))),
                $matches,
            )],
            scroll: 15,
        );

        return $chosen === '' ? null : (string) $chosen;
    }

    private function destination(Connection $connection, array $tables): ?string
    {
        $sql = $this->option('sql') ?? ($this->asked ? $this->askWhere($connection, $tables) : null);

        if ($sql === null) {
            return null;
        }

        $sql = Paths::expand($sql);

        if (is_dir($sql)) {
            $label = count($tables) === 1 ? $tables[0] : 'all';

            return rtrim($sql, '/').'/'.basename($this->exporter->filename($connection, $label));
        }

        return $sql;
    }
}
