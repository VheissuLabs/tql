<?php

namespace App\Dump;

use App\Models\Connection;

class Manifest
{
    public const FILE = 'tql.json';

    public static function write(string $directory, Engine $engine, Connection $connection, DumpOptions $options): void
    {
        file_put_contents($directory.'/'.self::FILE, json_encode([
            'tql' => config('app.version'),
            'engine' => $engine->name(),
            'tool' => $engine->dumper() ?? 'sqlite backup',
            'connection' => $connection->name,
            'database' => $connection->driver === 'sqlite'
                ? basename((string) $connection->database)
                : $connection->activeDatabase(),
            'tables' => $options->tables === []
                ? null
                : $options->tables,
            'data_only' => $options->dataOnly,
            'dumped_at' => date('c'),
        ], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES)."\n");
    }

    public static function read(string $directory): ?array
    {
        $path = $directory.'/'.self::FILE;

        if (! is_file($path)) {
            return null;
        }

        $manifest = json_decode((string) file_get_contents($path), true);

        return is_array($manifest)
            ? $manifest
            : null;
    }
}
