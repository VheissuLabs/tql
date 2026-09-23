<?php

namespace App\Database;

use App\Support\Now;

class ColumnDefault
{
    public const NONE = 'none';

    public const VALUE = 'value';

    public const NOW = 'now';

    public const EXPRESSION = 'expression';

    private const TIME = '/\b(current_timestamp|current_date|current_time|localtimestamp|localtime|getdate|getutcdate|sysdatetime|utc_timestamp)\b|\bnow\s*\(|\'now\'/i';

    public static function read(mixed $default, string $driver): array
    {
        if ($default === null) {
            return [self::NONE, null];
        }

        $text = self::unwrap(trim((string) $default));
        $literal = self::uncast($text);

        if ($text === '' || strcasecmp($literal, 'null') === 0) {
            return [self::NONE, null];
        }

        if (preg_match(self::TIME, $text) === 1) {
            return [self::NOW, Now::LITERAL];
        }

        if (preg_match("/^N?'((?:[^']|'')*)'$/s", $literal, $match) === 1) {
            return [self::VALUE, str_replace("''", "'", $match[1])];
        }

        if ($driver === 'sqlite' && preg_match('/^"((?:[^"]|"")*)"$/s', $literal, $match) === 1) {
            return [self::VALUE, str_replace('""', '"', $match[1])];
        }

        if (is_numeric($literal) || in_array(strtolower($literal), ['true', 'false'], true)) {
            return [self::VALUE, $literal];
        }

        if ($driver === 'mysql' && ! str_contains($text, '(')) {
            return [self::VALUE, (string) $default];
        }

        return [self::EXPRESSION, $text];
    }

    private static function unwrap(string $text): string
    {
        while (str_starts_with($text, '(') && self::closes($text) === mb_strlen($text) - 1) {
            $text = trim(mb_substr($text, 1, -1));
        }

        return $text;
    }

    private static function closes(string $text): int
    {
        $depth = 0;

        foreach (mb_str_split($text) as $at => $char) {
            $depth += match ($char) {
                '(' => 1,
                ')' => -1,
                default => 0,
            };

            if ($depth === 0) {
                return $at;
            }
        }

        return -1;
    }

    private static function uncast(string $text): string
    {
        return (string) preg_replace('/::[\w\s"\[\]]+$/', '', $text);
    }
}
