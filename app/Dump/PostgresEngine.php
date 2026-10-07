<?php

namespace App\Dump;

use App\Database\ConnectionManager;
use App\Models\Connection;
use App\Support\Paths;
use Symfony\Component\Process\Process;

class PostgresEngine implements Engine
{
    public function __construct(
        private ConnectionManager $connections,
        private Secrets $secrets,
    ) {}

    public function name(): string
    {
        return 'pgsql';
    }

    public function dumper(): ?string
    {
        return 'pg_dump';
    }

    public function loader(): ?string
    {
        return 'pg_restore';
    }

    public function install(): string
    {
        return PHP_OS_FAMILY === 'Darwin'
            ? 'brew install libpq && brew link --force libpq'
            : 'install the postgresql-client package from your package manager';
    }

    public function dump(Connection $connection, string $directory, DumpOptions $options): Job
    {
        $command = [
            'pg_dump',
            ...$this->target($connection),
            '--format=directory',
            '--jobs='.$options->threads,
            '--file='.$directory,
            '--verbose',
        ];

        foreach ($options->tables as $table) {
            $command[] = '--table='.$table;
        }

        if ($options->dataOnly) {
            $command[] = '--data-only';
        }

        return new Job(
            $command,
            $this->environment($connection),
            progress: Progress::countingTables('/dumping contents of table "([^"]+)"/'),
            tables: fn () => $this->tableCount($connection, $options->tables),
        );
    }

    public function load(Connection $connection, string $directory, LoadOptions $options): Job
    {
        $command = [
            'pg_restore',
            ...$this->target($connection),
            '--format=directory',
            '--jobs='.$options->threads,
            '--no-owner',
            '--no-privileges',
            '--exit-on-error',
            '--verbose',
        ];

        if ($options->drop) {
            $command[] = '--clean';
            $command[] = '--if-exists';
        }

        $command[] = $directory;

        return new Job(
            $command,
            $this->environment($connection),
            progress: Progress::countingTables('/processing data for table "([^"]+)"/'),
            tables: fn () => $this->tablesIn($directory),
        );
    }

    public function estimatedBytes(Connection $connection, array $tables): ?int
    {
        $database = $this->connections->resolve($connection);

        if ($tables === []) {
            return (int) $database->selectOne('select pg_database_size(current_database()) as bytes')->bytes;
        }

        $sizes = implode(' + ', array_fill(0, count($tables), 'pg_table_size(?::regclass)'));

        return (int) $database->selectOne("select {$sizes} as bytes", $tables)->bytes;
    }

    private function tableCount(Connection $connection, array $tables): int
    {
        if ($tables !== []) {
            return count($tables);
        }

        return (int) $this->connections->resolve($connection)->selectOne(
            "select count(*) as tables from information_schema.tables where table_schema not in ('pg_catalog', 'information_schema') and table_type = 'BASE TABLE'",
        )->tables;
    }

    private function tablesIn(string $directory): int
    {
        $listing = new Process(['pg_restore', '--list', $directory]);
        $listing->run();

        return substr_count($listing->getOutput(), ' TABLE DATA ');
    }

    public function compresses(): bool
    {
        return true;
    }

    public function recognises(string $directory): bool
    {
        return is_file($directory.'/toc.dat');
    }

    private function target(Connection $connection): array
    {
        $endpoint = Endpoint::of($connection);

        return [
            '--host='.$endpoint->host,
            '--port='.$endpoint->port,
            '--username='.$connection->username,
            '--dbname='.$connection->activeDatabase(),
            '--no-password',
        ];
    }

    private function environment(Connection $connection): array
    {
        $password = addcslashes((string) $connection->password, ':\\');

        return array_filter([
            'PGPASSFILE' => $this->secrets->write('pgpass', "*:*:*:*:{$password}\n"),
            'PGSSLMODE' => trim((string) $connection->ssl_mode),
            'PGSSLROOTCERT' => Paths::expand((string) $connection->ssl_ca),
            'PGSSLCERT' => Paths::expand((string) $connection->ssl_cert),
            'PGSSLKEY' => Paths::expand((string) $connection->ssl_key),
        ], fn (string $value) => $value !== '');
    }
}
