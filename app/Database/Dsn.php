<?php

namespace App\Database;

class Dsn
{
    private const DRIVERS = [
        'mysql' => 'mysql',
        'mariadb' => 'mysql',
        'pgsql' => 'pgsql',
        'postgres' => 'pgsql',
        'postgresql' => 'pgsql',
        'sqlsrv' => 'sqlsrv',
        'mssql' => 'sqlsrv',
        'sqlite' => 'sqlite',
    ];

    private const PORTS = [
        'mysql' => 3306,
        'pgsql' => 5432,
        'sqlsrv' => 1433,
    ];

    public static function looksLikeOne(string $value): bool
    {
        return preg_match('#^[a-z][a-z0-9+.-]*://#i', $value) === 1;
    }

    /**
     * Turn a connection string into connection attributes.
     *
     * @return array<string, mixed>|null null when it is not one we understand
     */
    public static function parse(string $dsn): ?array
    {
        if (! static::looksLikeOne($dsn)) {
            return null;
        }

        [$scheme] = explode('://', $dsn, 2);

        $driver = self::DRIVERS[strtolower($scheme)] ?? null;

        if ($driver === null) {
            return null;
        }

        // parse_url cannot read sqlite:///tmp/app.db at all, and a file path
        // has no host or credentials to pull apart anyway.
        if ($driver === 'sqlite') {
            return static::sqlite(substr($dsn, strlen($scheme) + 3));
        }

        $parts = parse_url($dsn);

        if ($parts === false) {
            return null;
        }

        $query = [];
        parse_str($parts['query'] ?? '', $query);

        $database = ltrim($parts['path'] ?? '', '/');

        return [
            'name' => static::name($query, $parts, $database),
            'driver' => $driver,
            'host' => $parts['host'] ?? '127.0.0.1',
            'port' => $parts['port'] ?? self::PORTS[$driver],
            'database' => $database === '' ? null : rawurldecode($database),
            'username' => isset($parts['user']) ? rawurldecode($parts['user']) : null,
            'password' => isset($parts['pass']) ? rawurldecode($parts['pass']) : null,
        ];
    }

    /**
     * @return array<string, mixed>
     */
    private static function sqlite(string $rest): array
    {
        [$path, $queryString] = array_pad(explode('?', $rest, 2), 2, '');

        $query = [];
        parse_str($queryString, $query);

        $path = rawurldecode($path);

        return [
            'name' => $query['name'] ?? basename($path),
            'driver' => 'sqlite',
            'database' => $path,
        ];
    }

    private static function name(array $query, array $parts, string $database): string
    {
        if (isset($query['name']) && $query['name'] !== '') {
            return (string) $query['name'];
        }

        $host = $parts['host'] ?? 'localhost';

        return $database === '' ? $host : $database.' on '.$host;
    }
}
