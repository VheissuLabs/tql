<?php

namespace App\Dump;

use App\Models\Connection;
use App\Support\Paths;
use RuntimeException;
use SQLite3;

class SqliteEngine implements Engine
{
    public const FILE = 'database.sqlite';

    public function name(): string
    {
        return 'sqlite';
    }

    public function dumper(): ?string
    {
        return null;
    }

    public function loader(): ?string
    {
        return null;
    }

    public function install(): string
    {
        return '';
    }

    public function dump(Connection $connection, string $directory, DumpOptions $options): Job
    {
        if ($options->tables !== [] || $options->dataOnly) {
            throw new RuntimeException('A SQLite dump is the whole file; it takes no tables and no --data-only.');
        }

        return Job::native(function () use ($connection, $directory) {
            if (! is_dir($directory)) {
                mkdir($directory, 0755, true);
            }

            $this->copy(Paths::expand((string) $connection->database), $directory.'/'.self::FILE);
        });
    }

    public function load(Connection $connection, string $directory, LoadOptions $options): Job
    {
        $target = Paths::expand((string) $connection->database);

        if (! $options->drop && is_file($target) && filesize($target) > 0) {
            throw new RuntimeException("{$connection->name} already has a database at {$target}; --drop replaces it.");
        }

        return Job::native(fn () => $this->copy($directory.'/'.self::FILE, $target));
    }

    public function estimatedBytes(Connection $connection, array $tables): ?int
    {
        $path = Paths::expand((string) $connection->database);

        return is_file($path)
            ? (int) filesize($path)
            : null;
    }

    public function compresses(): bool
    {
        return false;
    }

    public function recognises(string $directory): bool
    {
        return is_file($directory.'/'.self::FILE);
    }

    private function copy(string $from, string $to): void
    {
        $source = new SQLite3($from, SQLITE3_OPEN_READONLY);
        $destination = new SQLite3($to);

        try {
            if (! $source->backup($destination)) {
                throw new RuntimeException('SQLite could not copy the database: '.$source->lastErrorMsg());
            }
        } finally {
            $source->close();
            $destination->close();
        }
    }
}
