<?php

namespace App\Dump;

class Engines
{
    private const BY_DRIVER = [
        'mysql' => MySqlEngine::class,
        'pgsql' => PostgresEngine::class,
        'sqlite' => SqliteEngine::class,
    ];

    public static function for(string $driver): ?Engine
    {
        $class = self::BY_DRIVER[$driver] ?? null;

        return $class === null
            ? null
            : app($class);
    }

    public static function recognise(string $directory): ?Engine
    {
        $named = Manifest::read($directory)['engine'] ?? null;

        if ($named !== null) {
            return static::for($named);
        }

        foreach (array_keys(self::BY_DRIVER) as $driver) {
            $engine = static::for($driver);

            if ($engine->recognises($directory)) {
                return $engine;
            }
        }

        return null;
    }

    public static function label(string $driver): string
    {
        return match ($driver) {
            'mysql' => 'MySQL',
            'pgsql' => 'Postgres',
            'sqlite' => 'SQLite',
            'sqlsrv' => 'SQL Server',
            default => $driver,
        };
    }
}
