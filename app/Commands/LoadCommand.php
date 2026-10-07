<?php

namespace App\Commands;

use App\Commands\Concerns\RunsDumpTools;
use App\Connections\Tag;
use App\Database\ConnectionManager;
use App\Dump\Engines;
use App\Dump\LoadOptions;
use App\Dump\Manifest;
use App\Dump\StatementReader;
use App\Models\Connection;
use App\Support\Paths;
use LaravelZero\Framework\Commands\Command;
use Throwable;

use function Laravel\Prompts\confirm;

class LoadCommand extends Command
{
    use RunsDumpTools;

    protected $signature = 'load
        {source : A folder from tql dump, or a .sql file from tql export}
        {connection : The tql connection to load it into}
        {--database= : Which database on the server, for a connection that names none}
        {--drop : Drop and recreate tables that already exist}
        {--threads=4 : How many tables, or pieces of a table, to load at once}
        {--force : Load into a production connection without asking}
        {--dry-run : Show the command tql would run, and stop}';

    protected $aliases = ['import'];

    protected $description = 'Load a dump folder or an exported .sql file into a database';

    protected $help = <<<'HELP'
    Reads back what <fg=green>tql dump</> wrote, with myloader for MySQL and pg_restore for
    Postgres, or replays a <fg=green>tql export</> file in one transaction.

    Tables that already exist stop the load, unless <fg=green>--drop</> says to replace them.

    <fg=yellow>Examples</>

      <fg=green>tql load ~/.config/tql/exports/notarydash-20261007-091200 local</>
          a dump into the local connection

      <fg=green>tql load ./nd local --drop</>
          the same, replacing tables local already has

      <fg=green>tql load ./orders.sql local</>
          an export's rows into tables that exist
    HELP;

    public function __construct(private ConnectionManager $connections)
    {
        parent::__construct();
    }

    public function handle(): int
    {
        $source = rtrim(Paths::resolve($this->argument('source')), '/');

        if (! file_exists($source)) {
            $this->error("There is nothing at {$source}.");

            return self::FAILURE;
        }

        $connection = $this->findConnection($this->argument('connection'));

        if ($connection === null || ! $this->settleDatabase($connection) || ! $this->mayWriteTo($connection)) {
            return self::FAILURE;
        }

        return is_file($source)
            ? $this->replay($source, $connection)
            : $this->restore($source, $connection);
    }

    private function mayWriteTo(Connection $connection): bool
    {
        if ($connection->read_only) {
            $this->error("{$connection->name} is read only, so nothing can be loaded into it.");

            return false;
        }

        if (Tag::parse($connection->tag) !== Tag::Production || $this->option('force') || $this->option('dry-run')) {
            return true;
        }

        if (! $this->input->isInteractive()) {
            $this->error("{$connection->name} is tagged production; --force loads into it anyway.");

            return false;
        }

        return confirm(
            label: "{$connection->name} is tagged production. Load into it anyway?",
            default: false,
        );
    }

    private function restore(string $directory, Connection $connection): int
    {
        $source = Engines::recognise($directory);

        if ($source === null) {
            $this->error("{$directory} does not look like a dump from tql dump, mydumper or pg_dump.");

            return self::FAILURE;
        }

        $engine = $this->engineFor($connection);

        if ($engine === null) {
            return self::FAILURE;
        }

        if ($engine->name() !== $source->name()) {
            $this->error(sprintf(
                'That dump is from %s and %s is %s; tql does not move between engines yet.',
                Engines::label($source->name()),
                $connection->name,
                Engines::label($engine->name()),
            ));

            return self::FAILURE;
        }

        if (! $this->toolIsInstalled($engine->loader(), $engine)) {
            return self::FAILURE;
        }

        if ($connection->driver !== 'sqlite' && $failure = $this->connections->test($this->serverOf($connection))) {
            $this->error($failure);

            return self::FAILURE;
        }

        $options = new LoadOptions(
            drop: (bool) $this->option('drop'),
            threads: max(1, (int) $this->option('threads')),
        );

        try {
            $job = $engine->load($connection, $directory, $options);
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());

            return self::FAILURE;
        }

        $manifest = Manifest::read($directory);

        $this->line(sprintf(
            '  Loading %s into <fg=cyan>%s</>%s',
            $manifest === null
                ? Paths::shorten($directory)
                : ($manifest['database'] ?? 'the dump').' from '.($manifest['connection'] ?? 'somewhere'),
            $connection->name,
            $options->drop
                ? ', replacing tables it already has'
                : '',
        ));

        if ($this->option('dry-run')) {
            $this->line('  '.($job->native === null ? $job->describe() : 'a SQLite restore into '.$connection->database));

            return self::SUCCESS;
        }

        $started = microtime(true);

        if (! $this->runQuietlyFailing(fn () => $this->runJob($job))) {
            $this->error('The load did not finish.');

            if (! $options->drop) {
                $this->line('  if a table already exists, <fg=green>--drop</> replaces it.');
            }

            return self::FAILURE;
        }

        $this->line('  <fg=green>Loaded</> in '.$this->elapsed($started));

        return self::SUCCESS;
    }

    private function serverOf(Connection $connection): Connection
    {
        if ($connection->driver !== 'mysql') {
            return $connection;
        }

        $server = clone $connection;
        $server->sessionDatabase = '';

        return $server;
    }

    private function replay(string $file, Connection $connection): int
    {
        if ($this->option('drop')) {
            $this->error('--drop is for dump folders; a .sql file from tql export goes into tables that exist.');

            return self::FAILURE;
        }

        if ($failure = $this->connections->test($connection)) {
            $this->error($failure);

            return self::FAILURE;
        }

        $this->line('  Replaying '.Paths::shorten($file).' into <fg=cyan>'.$connection->name.'</>, in one transaction');

        if ($this->option('dry-run')) {
            return self::SUCCESS;
        }

        $started = microtime(true);
        $pdo = $this->connections->resolve($connection)->getPdo();
        $statements = 0;

        $pdo->beginTransaction();

        try {
            foreach ((new StatementReader($connection->driver === 'mysql'))->read($file) as $statement) {
                $pdo->exec($statement);
                $statements++;
            }

            $pdo->commit();
        } catch (Throwable $exception) {
            $pdo->rollBack();

            $this->error('Statement '.($statements + 1).' failed, so nothing was loaded: '.$exception->getMessage());

            return self::FAILURE;
        }

        $this->line(sprintf('  <fg=green>Loaded</> %d statements in %s', $statements, $this->elapsed($started)));

        return self::SUCCESS;
    }
}
