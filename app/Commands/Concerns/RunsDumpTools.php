<?php

namespace App\Commands\Concerns;

use App\Dump\Engine;
use App\Dump\Engines;
use App\Dump\Job;
use App\Models\Connection;
use App\Ssh\Tunnel;
use Symfony\Component\Process\Exception\ProcessSignaledException;
use Symfony\Component\Process\ExecutableFinder;
use Symfony\Component\Process\Process;
use Throwable;

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

    private function runJob(Job $job): bool
    {
        if ($job->native !== null) {
            ($job->native)();

            return true;
        }

        $this->stopCleanlyOnInterrupt();

        $process = new Process($job->command, null, $job->environment === [] ? null : $job->environment);
        $process->setTimeout(null);

        try {
            $exitCode = $process->run(function (string $type, string $output) {
                foreach (preg_split('/\R/', rtrim($output)) ?: [] as $line) {
                    $this->line('  <fg=gray>'.$line.'</>');
                }
            });
        } catch (ProcessSignaledException) {
            $exitCode = 1;
        } finally {
            Tunnel::closeAll();
        }

        return $exitCode === 0;
    }

    private function stopCleanlyOnInterrupt(): void
    {
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
        $seconds = (int) round(microtime(true) - $started);

        return $seconds < 60
            ? $seconds.'s'
            : intdiv($seconds, 60).'m '.($seconds % 60).'s';
    }
}
