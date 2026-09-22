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
        {--tag= : what to call it in the connection list}
        {--peek : open it without remembering it}';

    protected $description = 'Open a database by path or connection string';

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

        $tag = $this->option('tag');
        $existing = static::matching($attributes);

        if ($existing !== null) {
            // Re-tagging an existing connection renames it rather than making
            // a second one pointing at the same database.
            if ($tag !== null && $tag !== $existing->name) {
                $existing->forceFill(['name' => static::freeName($tag)])->save();
            }

            return $existing;
        }

        $attributes['name'] = $tag ?: $name;

        // Remembered by default: the point of opening by connection string is
        // to not have to find it again.
        if ($this->option('peek')) {
            return new Connection($attributes);
        }

        $attributes['name'] = static::freeName($attributes['name']);

        return Connection::create($attributes);
    }

    /**
     * Connection names are unique, and two projects both called database.sqlite
     * is the normal case rather than the exception, so number the duplicates
     * instead of failing on the constraint.
     */
    private static function freeName(string $name): string
    {
        if (! Connection::where('name', $name)->exists()) {
            return $name;
        }

        for ($suffix = 2; $suffix < 1000; $suffix++) {
            if (! Connection::where('name', "{$name} ({$suffix})")->exists()) {
                return "{$name} ({$suffix})";
            }
        }

        return $name.' ('.uniqid().')';
    }

    /**
     * Find a saved connection pointing at the same place.
     *
     * Matched on where it points, never on the password: that column has an
     * encrypted cast, and the ciphertext differs every time it is written, so
     * comparing against it would never match and --save would pile up a
     * duplicate on every run.
     *
     * @param  array<string, mixed>  $attributes
     */
    private static function matching(array $attributes): ?Connection
    {
        $identity = array_filter(
            array_intersect_key($attributes, array_flip([
                'driver', 'host', 'port', 'database', 'username',
            ])),
            fn ($value) => $value !== null,
        );

        return Connection::where($identity)->first();
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
