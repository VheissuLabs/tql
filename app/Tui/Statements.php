<?php

namespace App\Tui;

final class Statements
{
    public static function at(string $sql, int $cursor): ?array
    {
        $statements = self::split($sql);

        if ($statements === []) {
            return null;
        }

        $number = count($statements);

        foreach ($statements as $index => $statement) {
            if ($cursor <= $statement['end']) {
                $number = $index + 1;

                break;
            }
        }

        return $statements[$number - 1] + ['number' => $number, 'of' => count($statements)];
    }

    private static function split(string $sql): array
    {
        $chars = mb_str_split($sql);
        $length = count($chars);
        $statements = [];
        $start = 0;
        $at = 0;

        while ($at < $length) {
            $skipped = self::skipQuoted($chars, $at);

            if ($skipped !== $at) {
                $at = $skipped;

                continue;
            }

            if ($chars[$at] === ';') {
                $statements[] = self::statement($chars, $start, $at, $at + 1);
                $start = $at + 1;
            }

            $at++;
        }

        $statements[] = self::statement($chars, $start, $length, null);

        return array_values(array_filter($statements));
    }

    private static function statement(array $chars, int $start, int $end, ?int $terminator): ?array
    {
        $body = implode('', array_slice($chars, $start, $end - $start));
        $sql = trim($body);

        if ($sql === '') {
            return null;
        }

        return [
            'sql' => $sql,
            'start' => $start + mb_strlen($body) - mb_strlen(ltrim($body)),
            'end' => $terminator ?? $start + mb_strlen(rtrim($body)),
        ];
    }

    private static function skipQuoted(array $chars, int $at): int
    {
        $char = $chars[$at];
        $next = $chars[$at + 1] ?? '';

        return match (true) {
            $char === '-' && $next === '-' => self::skipUntil($chars, $at + 2, "\n"),
            $char === '/' && $next === '*' => self::skipUntil($chars, $at + 2, '*/'),
            $char === "'", $char === '"', $char === '`' => self::skipString($chars, $at, $char),
            $char === '$' => self::skipDollarQuote($chars, $at),
            default => $at,
        };
    }

    private static function skipString(array $chars, int $at, string $quote): int
    {
        $at++;

        while ($at < count($chars)) {
            if ($chars[$at] === $quote && ($chars[$at + 1] ?? '') === $quote) {
                $at += 2;

                continue;
            }

            if ($chars[$at] === $quote) {
                return $at + 1;
            }

            $at++;
        }

        return $at;
    }

    private static function skipDollarQuote(array $chars, int $at): int
    {
        $ahead = implode('', array_slice($chars, $at, 64));

        if (preg_match('/^\$([A-Za-z_][A-Za-z0-9_]*)?\$/', $ahead, $match) !== 1) {
            return $at;
        }

        return self::skipUntil($chars, $at + mb_strlen($match[0]), $match[0]);
    }

    private static function skipUntil(array $chars, int $at, string $closer): int
    {
        $closing = mb_str_split($closer);
        $width = count($closing);

        while ($at < count($chars)) {
            if (array_slice($chars, $at, $width) === $closing) {
                return $at + $width;
            }

            $at++;
        }

        return $at;
    }
}
