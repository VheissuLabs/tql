<?php

namespace App\Commands\Concerns;

use App\Connections\Tag;
use App\Dump\Engine;
use App\Dump\Engines;
use App\Dump\Job;
use App\Models\Connection;
use App\Ssh\Tunnel;
use App\Support\Paths;
use Laravel\Prompts\Progress;
use Symfony\Component\Process\Exception\ProcessSignaledException;
use Symfony\Component\Process\ExecutableFinder;
use Symfony\Component\Process\Process;
use Throwable;

use function Laravel\Prompts\confirm;

trait RunsDumpTools
{
    private function findConnection(string $name): ?Connection
    {
        $connection = Connection::where('name', $name)->first();

        if ($connection !== null) {
            return $connection;
        }

        $this->error("No connection named [{$name}].");
        $this->line('  known: '.Connection::orderBy('name')->pluck('name')->implode(', '));

        return null;
    }

    private function settleDatabase(Connection $connection): bool
    {
        if ($this->option('database') !== null) {
            $connection->sessionDatabase = $this->option('database');
        }

        if ($connection->driver === 'sqlite' || trim((string) $connection->activeDatabase()) !== '') {
            return true;
        }

        $this->error("[{$connection->name}] is a server, not a database.");
        $this->line('  name one with <fg=green>--database=</>.');

        return false;
    }

    private function engineFor(Connection $connection): ?Engine
    {
        $engine = Engines::for($connection->driver);

        if ($engine === null) {
            $this->error(Engines::label($connection->driver).' is not supported yet; tql dumps and loads MySQL, Postgres and SQLite.');
        }

        return $engine;
    }

    private function toolIsInstalled(?string $tool, Engine $engine): bool
    {
        if ($tool === null || (new ExecutableFinder)->find($tool) !== null) {
            return true;
        }

        $this->error("{$tool} is not installed, and tql uses it for ".Engines::label($engine->name()).'.');
        $this->line('  <fg=green>'.$engine->install().'</>');

        return false;
    }

    private function runJob(Job $job, string $label = 'Working'): bool
    {
        if ($job->native !== null) {
            ($job->native)();

            return true;
        }

        $this->stopCleanlyOnInterrupt();

        $process = new Process($job->command, null, $job->environment === [] ? null : $job->environment);
        $process->setTimeout(null);

        $verbose = $this->output->isVerbose();
        $live = ! $verbose && function_exists('stream_isatty') && @stream_isatty(STDOUT);
        $bar = $live
            ? $this->startProgress($label, $job->expectedTables())
            : null;
        $recent = [];

        try {
            $exitCode = $process->run(function (string $type, string $output) use ($job, $label, $verbose, $live, &$bar, &$recent) {
                foreach (preg_split('/\R/', rtrim($output)) ?: [] as $line) {
                    if (trim($line) === '') {
                        continue;
                    }

                    if ($verbose) {
                        $this->line('  <fg=gray>'.$line.'</>');

                        continue;
                    }

                    $recent = array_slice([...$recent, $line], -40);

                    if ($live && $job->progress !== null && ($update = ($job->progress)($line)) !== null) {
                        $bar = $this->showProgress($bar, $label, $update);
                    }
                }
            });
        } catch (ProcessSignaledException) {
            $exitCode = 1;
        }

        if ($bar !== null) {
            if ($exitCode === 0) {
                $bar->progress = $bar->total;
            }

            $bar->hint('');
            $bar->finish();
        }

        if ($exitCode !== 0 && ! $verbose) {
            $this->explainFailure($recent);
        }

        return $exitCode === 0;
    }

    private function startProgress(string $label, int $expected): ?Progress
    {
        if ($expected < 1) {
            return null;
        }

        $bar = new Progress($label, $expected, 'connecting…');
        $bar->start();

        return $bar;
    }

    private function showProgress(?Progress $bar, string $label, array $update): ?Progress
    {
        [$done, $total, $current] = $update;
        $total = $total ?: $bar?->total;

        if ($total === null || $total < 1) {
            return $bar;
        }

        $bar ??= $this->startProgress($label, $total);
        $bar->total = $total;
        $bar->progress = min($done, $total);
        $bar->hint((string) $current);
        $bar->render();

        return $bar;
    }

    private function explainFailure(array $recent): void
    {
        $problems = array_values(array_filter(
            $recent,
            fn (string $line) => preg_match('/warning|error|critical|fatal/i', $line) === 1,
        ));

        foreach (array_slice($problems ?: $recent, -8) as $line) {
            $this->line('  <fg=gray>'.trim($line).'</>');
        }

        $this->line('  run it again with <fg=green>-v</> to see everything the tool said.');
    }

    private function stopCleanlyOnInterrupt(): void
    {
        register_shutdown_function(Tunnel::closeAll(...));

        if (! function_exists('pcntl_signal')) {
            return;
        }

        pcntl_async_signals(true);

        foreach ([SIGINT, SIGTERM] as $signal) {
            pcntl_signal($signal, function () {
                Tunnel::closeAll();

                exit(130);
            });
        }
    }

    private function humanBytes(int $bytes): string
    {
        $units = ['B', 'KB', 'MB', 'GB', 'TB'];
        $power = $bytes > 0
            ? min((int) floor(log($bytes, 1024)), count($units) - 1)
            : 0;

        return round($bytes / (1024 ** $power), 1).' '.$units[$power];
    }

    private function directorySize(string $directory): int
    {
        $bytes = 0;

        $files = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS),
        );

        foreach ($files as $file) {
            $bytes += $file->getSize();
        }

        return $bytes;
    }

    private function runQuietlyFailing(\Closure $work): bool
    {
        try {
            return $work();
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());

            return false;
        }
    }

    private function elapsed(float $started): string
    {
        $seconds = microtime(true) - $started;

        return match (true) {
            $seconds < 10 => number_format($seconds, 1).'s',
            $seconds < 60 => ((int) round($seconds)).'s',
            default => intdiv((int) round($seconds), 60).'m '.(((int) round($seconds)) % 60).'s',
        };
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

    private function roomFor(?int $estimate, Engine $engine, string $directory, string $folderOption = '--to'): bool
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
        $this->line("  choose another folder with <fg=green>{$folderOption}=</>, or <fg=green>--force</> to try anyway.");

        return false;
    }

    private function nearestExisting(string $directory): string
    {
        while (! is_dir($directory) && dirname($directory) !== $directory) {
            $directory = dirname($directory);
        }

        return $directory;
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
}
