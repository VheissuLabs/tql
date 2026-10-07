<?php

namespace App\Commands;

use App\Commands\Concerns\RunsDumpTools;
use App\Database\ConnectionManager;
use App\Database\SqlExporter;
use App\Dump\DumpOptions;
use App\Dump\Engines;
use App\Dump\LoadOptions;
use App\Dump\Manifest;
use App\Models\Connection;
use App\Support\Paths;
use FilesystemIterator;
use LaravelZero\Framework\Commands\Command;
use RecursiveDirectoryIterator;
use RecursiveIteratorIterator;
use Throwable;

class SyncCommand extends Command
{
    use RunsDumpTools;

    protected $signature = 'sync
        {from : The connection to copy from, which is only ever read}
        {to : The connection to copy into}
        {tables?* : Only these tables, or every table when none are named}
        {--database= : The database to copy, for a connection that names none}
        {--into= : What the database is called on the other side, when it is not the same}
        {--threads=4 : How many tables, or pieces of a table, to copy at once}
        {--no-lock : Skip the consistent snapshot, for a MySQL user without the privileges it needs}
        {--via= : The folder the copy passes through on its way}
        {--keep : Keep that folder afterwards, as a backup}
        {--force : Copy into a production connection, or past the free space check}
        {--dry-run : Show both steps without running them}';

    protected $description = 'Make one database a copy of another: a dump and a load in one step';

    protected $help = <<<'HELP'
    Dumps a database from one connection and loads it into another, replacing
    the tables it brings over. Tables it does not bring over are left alone.

    <fg=yellow>Examples</>

      <fg=green>tql sync prod local --database=shop</>
          shop on prod becomes shop on local

      <fg=green>tql sync prod local --database=shop --into=shop_copy</>
          the same, under another name locally

      <fg=green>tql sync prod local --database=shop users orders</>
          just two tables
    HELP;

    public function __construct(private ConnectionManager $connections, private SqlExporter $exporter)
    {
        parent::__construct();
    }

    public function handle(): int
    {
        $source = $this->findConnection($this->argument('from'));
        $target = $source === null
            ? null
            : $this->findConnection($this->argument('to'));

        if ($source === null || $target === null || ! $this->settleDatabase($source)) {
            return self::FAILURE;
        }

        $target->sessionDatabase = $this->option('into')
            ?? (trim((string) $target->database) ?: $source->activeDatabase());

        $engine = $this->engineFor($source);

        if ($engine === null) {
            return self::FAILURE;
        }

        if (Engines::for($target->driver)?->name() !== $engine->name()) {
            $this->error(sprintf(
                '%s is %s and %s is %s; tql does not move between engines yet.',
                $source->name,
                Engines::label($source->driver),
                $target->name,
                Engines::label($target->driver),
            ));

            return self::FAILURE;
        }

        if ($this->samePlace($source, $target)) {
            $this->error("That would copy {$this->label($source)} onto itself.");

            return self::FAILURE;
        }

        if (! $this->mayWriteTo($target)) {
            return self::FAILURE;
        }

        if (! $this->toolIsInstalled($engine->dumper(), $engine) || ! $this->toolIsInstalled($engine->loader(), $engine)) {
            return self::FAILURE;
        }

        $reachable = $target->driver === 'sqlite'
            ? [$source]
            : [$source, $this->serverOf($target)];

        foreach ($reachable as $connection) {
            if ($failure = $this->connections->test($connection)) {
                $this->error("{$connection->name}: {$failure}");

                return self::FAILURE;
            }
        }

        $directory = $this->directory($source);
        $dumpOptions = new DumpOptions(
            tables: $this->argument('tables'),
            threads: max(1, (int) $this->option('threads')),
            noLock: (bool) $this->option('no-lock'),
        );
        $loadOptions = new LoadOptions(drop: true, threads: $dumpOptions->threads);

        try {
            $dump = $engine->dump($source, $directory, $dumpOptions);
            $load = $engine->load($target, $directory, $loadOptions);
            $estimate = $engine->estimatedBytes($source, $dumpOptions->tables);
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());

            return self::FAILURE;
        }

        if (! $this->option('force') && ! $this->roomFor($estimate, $engine, $directory, '--via')) {
            return self::FAILURE;
        }

        $this->line(sprintf(
            '  Syncing <fg=cyan>%s</> into <fg=cyan>%s</>%s',
            $this->label($source),
            $this->label($target),
            $estimate === null
                ? ''
                : ' (about '.$this->humanBytes($estimate).' of data)',
        ));

        if ($this->option('dry-run')) {
            foreach ([$dump, $load] as $job) {
                $this->line('  '.($job->native === null ? $job->describe() : 'a SQLite copy'));
            }

            return self::SUCCESS;
        }

        if (! is_dir(dirname($directory))) {
            mkdir(dirname($directory), 0755, true);
        }

        $started = microtime(true);

        if (! $this->runQuietlyFailing(fn () => $this->runJob($dump, 'Dumping'))) {
            $this->forget($directory);
            $this->error("The copy out of {$source->name} did not finish, so {$target->name} was not touched.");

            return self::FAILURE;
        }

        Manifest::write($directory, $engine, $source, $dumpOptions);

        if (! $this->runQuietlyFailing(fn () => $this->runJob($load, 'Loading'))) {
            $this->error("Loading into {$target->name} did not finish.");
            $this->line('  the dump is still in '.Paths::shorten($directory).'; <fg=green>tql load</> can try it again.');

            return self::FAILURE;
        }

        $this->option('keep')
            ? $this->line('  the dump is kept in '.Paths::shorten($directory))
            : $this->forget($directory);

        $this->line('  <fg=green>Synced</> in '.$this->elapsed($started));

        return self::SUCCESS;
    }

    private function samePlace(Connection $source, Connection $target): bool
    {
        $where = fn (Connection $connection) => $connection->driver === 'sqlite'
            ? [Paths::expand((string) $connection->database)]
            : [$connection->host, (int) $connection->port, (string) $connection->ssh_host, $connection->activeDatabase()];

        return $source->driver === $target->driver && $where($source) === $where($target);
    }

    private function label(Connection $connection): string
    {
        return $connection->driver === 'sqlite'
            ? $connection->name
            : $connection->name.' · '.$connection->activeDatabase();
    }

    private function directory(Connection $source): string
    {
        if ($this->option('via') !== null) {
            return rtrim(Paths::resolve($this->option('via')), '/');
        }

        $slug = strtolower(preg_replace('/[^A-Za-z0-9]+/', '-', (string) ($source->activeDatabase() ?: $source->name)) ?: 'sync');

        return $this->exporter->directory().'/sync-'.$slug.'-'.date('Ymd-His');
    }

    private function forget(string $directory): void
    {
        if (! is_dir($directory)) {
            return;
        }

        $entries = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($directory, FilesystemIterator::SKIP_DOTS),
            RecursiveIteratorIterator::CHILD_FIRST,
        );

        foreach ($entries as $entry) {
            $entry->isDir()
                ? rmdir($entry->getPathname())
                : unlink($entry->getPathname());
        }

        rmdir($directory);
    }
}
