<?php

namespace App\Support;

class Argv
{
    /**
     * Let `dotsql some.sqlite` mean `dotsql open some.sqlite`.
     *
     * Laravel Zero reads the first argument as a command name, so without
     * this a path is met with "command not found". A file that exists on
     * disk is never a command name, which makes the rewrite unambiguous.
     *
     * @param  array<int, string>  $argv
     * @return array<int, string>
     */
    public static function rewrite(array $argv): array
    {
        $first = $argv[1] ?? null;

        if ($first === null || str_starts_with($first, '-') || ! static::isPath($first)) {
            return $argv;
        }

        array_splice($argv, 1, 0, 'open');

        return $argv;
    }

    /**
     * A file that exists is never a command name. A path-shaped argument that
     * does not exist is routed too, so the user gets "No such file" instead
     * of Symfony's "No arguments expected".
     */
    private static function isPath(string $argument): bool
    {
        return is_file($argument)
            || str_contains($argument, '/')
            || preg_match('/\.(sqlite3?|db)$/i', $argument) === 1;
    }
}
