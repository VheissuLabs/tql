<?php

namespace App\Commands;

use App\Commands\Concerns\RunsDumpTools;
use App\Database\ConnectionManager;
use App\Database\SqlExporter;
use App\Dump\DumpOptions;
use App\Dump\Engine;
use App\Dump\Manifest;
use App\Models\Connection;
use App\Support\Paths;
use LaravelZero\Framework\Commands\Command;
use Throwable;

class DumpCommand extends Command
{
    use RunsDumpTools;

    protected $signature = 'dump
        {connection : The tql connection to dump}
        {tables?* : Only these tables, or every table when none are named}
        {--to= : The folder to write the dump into}
        {--database= : Which database on the server, for a connection that names none}
        {--threads=4 : How many tables, or pieces of a table, to dump at once}
        {--data-only : Rows only, for loading into tables that already exist}
        {--no-lock : Skip the consistent snapshot, for a MySQL user without the privileges it needs}
        {--force : Dump even when the drive looks too small}
        {--dry-run : Show the command tql would run, and stop}';

    protected $description = 'Dump a database, schema and data, with mydumper or pg_dump';

    protected $help = <<<'HELP'
    Writes a whole database, or some of its tables, into a folder that
    <fg=green>tql load</> reads back. MySQL uses mydumper and Postgres uses pg_dump, in
    parallel and from a consistent snapshot. SQLite needs no tool.

    <fg=yellow>Examples</>

      <fg=green>tql dump prod</>
          every table, into a new folder next to your exports

      <fg=green>tql dump prod orders users --to=./orders</>
          two tables, into a folder you name

      <fg=green>tql dump prod --data-only</>
          rows only, to load into a database that already has the tables
    HELP;

    public function __construct(private ConnectionManager $connections, private SqlExporter $exporter)
    {
        parent::__construct();
    }

    public function handle(): int
    {
        $connection = $this->findConnection($this->argument('connection'));

        if ($connection === null || ! $this->settleDatabase($connection)) {
            return self::FAILURE;
        }

        $engine = $this->engineFor($connection);

        if ($engine === null || ! $this->toolIsInstalled($engine->dumper(), $engine)) {
            return self::FAILURE;
        }

        $directory = $this->directory($connection);

        if (is_dir($directory) && (new \FilesystemIterator($directory))->valid()) {
            $this->error("{$directory} already has something in it; dump into a new or empty folder.");

            return self::FAILURE;
        }

        if ($failure = $this->connections->test($connection)) {
            $this->error($failure);

            return self::FAILURE;
        }

        $options = new DumpOptions(
            tables: $this->argument('tables'),
            threads: max(1, (int) $this->option('threads')),
            dataOnly: (bool) $this->option('data-only'),
            noLock: (bool) $this->option('no-lock'),
        );

        try {
            $job = $engine->dump($connection, $directory, $options);
            $estimate = $engine->estimatedBytes($connection, $options->tables);
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());

            return self::FAILURE;
        }

        if (! $this->option('force') && ! $this->roomFor($estimate, $engine, $directory)) {
            return self::FAILURE;
        }

        $this->line(sprintf(
            '  Dumping <fg=cyan>%s</>%s into %s',
            $connection->name,
            $estimate === null
                ? ''
                : ' (about '.$this->humanBytes($estimate).' of data)',
            Paths::shorten($directory),
        ));

        if ($this->option('dry-run')) {
            $this->line('  '.($job->native === null ? $job->describe() : 'a SQLite backup of '.$connection->database));

            return self::SUCCESS;
        }

        if (! is_dir(dirname($directory))) {
            mkdir(dirname($directory), 0755, true);
        }

        $started = microtime(true);

        if (! $this->runQuietlyFailing(fn () => $this->runJob($job))) {
            $this->error('The dump did not finish; what is in '.Paths::shorten($directory).' is incomplete.');

            return self::FAILURE;
        }

        Manifest::write($directory, $engine, $connection, $options);

        $this->line(sprintf(
            '  <fg=green>Dumped</> %s in %s',
            $this->humanBytes($this->directorySize($directory)),
            $this->elapsed($started),
        ));
        $this->line('  load it with <fg=green>tql load '.Paths::shorten($directory).' <connection></>');

        return self::SUCCESS;
    }

    private function directory(Connection $connection): string
    {
        if ($this->option('to') !== null) {
            return rtrim(Paths::resolve($this->option('to')), '/');
        }

        $subject = $connection->driver === 'sqlite'
            ? pathinfo((string) $connection->database, PATHINFO_FILENAME)
            : (string) $connection->activeDatabase();

        $slug = strtolower(preg_replace('/[^A-Za-z0-9]+/', '-', $subject ?: $connection->name) ?: 'dump');

        return $this->exporter->directory().'/'.$slug.'-'.date('Ymd-His');
    }

    private function roomFor(?int $estimate, Engine $engine, string $directory): bool
    {
        if ($estimate === null) {
            return true;
        }

        $needed = $engine->compresses()
            ? intdiv($estimate, 3)
            : $estimate;

        $free = disk_free_space($this->nearestExisting($directory));

        if ($free === false || $free >= $needed) {
            return true;
        }

        $this->error(sprintf(
            'The dump needs about %s and the drive under %s has %s free.',
            $this->humanBytes($needed),
            Paths::shorten($directory),
            $this->humanBytes((int) $free),
        ));
        $this->line('  choose another folder with <fg=green>--to=</>, or <fg=green>--force</> to try anyway.');

        return false;
    }

    private function nearestExisting(string $directory): string
    {
        while (! is_dir($directory) && dirname($directory) !== $directory) {
            $directory = dirname($directory);
        }

        return $directory;
    }
}
