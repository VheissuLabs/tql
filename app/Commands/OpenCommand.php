<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\error;

class OpenCommand extends Command
{
    protected $signature = 'open
        {path : path to a SQLite database file}
        {--save= : also remember it under this name}';

    protected $description = 'Open a SQLite file straight away, without saving a connection';

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

        if (! is_file($path)) {
            error("No such file: {$path}");

            return self::FAILURE;
        }

        if (! static::looksLikeSqlite($path)) {
            error(basename($path).' is not a SQLite database.');

            return self::FAILURE;
        }

        $connection = $this->connection((string) realpath($path));

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

    private function connection(string $path): Connection
    {
        $attributes = ['driver' => 'sqlite', 'database' => $path];

        $existing = Connection::where($attributes)->first();

        if ($existing !== null) {
            return $existing;
        }

        $name = $this->option('save') ?: basename($path);

        // Without --save the model is never persisted, so opening a file
        // does not quietly fill the connection list with one-off entries.
        return $this->option('save')
            ? Connection::create($attributes + ['name' => $name])
            : new Connection($attributes + ['name' => $name]);
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
