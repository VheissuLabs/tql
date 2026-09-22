<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\Dsn;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\error;

class OpenCommand extends Command
{
    protected $signature = 'open
        {path : a SQLite file, or a mysql:// pgsql:// sqlsrv:// connection string}
        {--save= : also remember it under this name}';

    protected $description = 'Open a database straight away, without saving a connection';

    public function __construct(
        private ConnectionManager $connections,
        private QueryRunner $runner,
        private RowFormatter $formatter,
    ) {
        parent::__construct();
    }

    public function handle(): int
    {
        $path = (string) $this->argument('path');

        return Dsn::looksLikeOne($path) ? $this->openDsn($path) : $this->openFile($path);
    }

    private function openDsn(string $dsn): int
    {
        $attributes = Dsn::parse($dsn);

        if ($attributes === null) {
            error('That connection string is not one I understand.');

            return self::FAILURE;
        }

        return $this->browse($this->connectionFor($attributes));
    }

    private function openFile(string $path): int
    {
        if (! is_file($path)) {
            error("No such file: {$path}");

            return self::FAILURE;
        }

        if (! static::looksLikeSqlite($path)) {
            error(basename($path).' is not a SQLite database.');

            return self::FAILURE;
        }

        return $this->browse($this->connectionFor([
            'name' => basename($path),
            'driver' => 'sqlite',
            'database' => (string) realpath($path),
        ]));
    }

    private function browse(Connection $connection): int
    {
        if ($failure = $this->connections->test($connection)) {
            error($failure);

            return self::FAILURE;
        }

        if ($connection->exists) {
            $this->connections->touch($connection);
        }

        (new Browser($connection, $this->runner, $this->formatter))->prompt();

        return self::SUCCESS;
    }

    /**
     * @param  array<string, mixed>  $attributes
     */
    private function connectionFor(array $attributes): Connection
    {
        $name = $attributes['name'];
        unset($attributes['name']);

        $existing = Connection::where(array_filter(
            $attributes,
            fn ($value) => $value !== null,
        ))->first();

        if ($existing !== null) {
            return $existing;
        }

        $attributes['name'] = $this->option('save') ?: $name;

        // Without --save the model is never persisted, so opening something
        // once does not quietly fill the connection list with one-off entries.
        return $this->option('save')
            ? Connection::create($attributes)
            : new Connection($attributes);
    }

    /**
     * Every SQLite file starts with this header, so we can say so plainly
     * rather than handing the user a PDO exception.
     */
    public static function looksLikeSqlite(string $path): bool
    {
        $handle = fopen($path, 'rb');

        if ($handle === false) {
            return false;
        }

        $header = fread($handle, 16);

        fclose($handle);

        return $header === "SQLite format 3\0";
    }
}
