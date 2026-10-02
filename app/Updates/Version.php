<?php

namespace App\Updates;

class Version
{
    public static function of(string $raw): ?string
    {
        return preg_match('/(\d+\.\d+\.\d+)/', $raw, $match) === 1
            ? $match[1]
            : null;
    }

    public static function isNewer(string $candidate, string $current): bool
    {
        return version_compare($candidate, $current, '>');
    }
}
