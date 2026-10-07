<?php

namespace App\Dump;

use Closure;

class Progress
{
    public static function countingTables(string $pattern): Closure
    {
        $seen = [];

        return function (string $line) use ($pattern, &$seen): ?array {
            if (preg_match($pattern, $line, $match) !== 1) {
                return null;
            }

            $seen[$match[1]] = true;

            return [count($seen), null, $match[1]];
        };
    }

    public static function reportedTables(string $countPattern, string $namePattern): Closure
    {
        $done = 0;
        $total = null;
        $name = null;

        return function (string $line) use ($countPattern, $namePattern, &$done, &$total, &$name): ?array {
            $named = preg_match($namePattern, $line, $nameMatch) === 1;
            $counted = preg_match($countPattern, $line, $countMatch) === 1;

            if (! $named && ! $counted) {
                return null;
            }

            if ($named) {
                $name = $nameMatch[1];
            }

            if ($counted) {
                $done = (int) $countMatch[1];
                $total = (int) $countMatch[2];
            }

            return [$done, $total, $name];
        };
    }
}
