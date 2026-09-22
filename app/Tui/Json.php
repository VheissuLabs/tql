<?php

namespace App\Tui;

class Json
{
    public static function looksLikeJson(?string $value): bool
    {
        if ($value === null) {
            return false;
        }

        $trimmed = trim($value);

        if ($trimmed === '' || ! in_array($trimmed[0], ['{', '['], true)) {
            return false;
        }

        json_decode($trimmed);

        return json_last_error() === JSON_ERROR_NONE;
    }

    public static function pretty(string $value): string
    {
        $decoded = json_decode(trim($value));

        if (json_last_error() !== JSON_ERROR_NONE) {
            return $value;
        }

        return json_encode($decoded, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    }

    public static function tokenise(string $line): array
    {
        $tokens = [];
        $pattern = '/("(?:\\\\.|[^"\\\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|(true|false|null)|([\[\]{},:])|(\s+)/';

        preg_match_all($pattern, $line, $matches, PREG_SET_ORDER);

        foreach ($matches as $match) {
            if (($match[1] ?? '') !== '') {
                $isKey = ($match[2] ?? '') !== '';

                $tokens[] = [$isKey ? 'key' : 'string', $match[1]];

                if ($isKey) {
                    $tokens[] = ['punctuation', $match[2]];
                }

                continue;
            }

            $tokens[] = match (true) {
                ($match[3] ?? '') !== '' => ['number', $match[3]],
                ($match[4] ?? '') !== '' => ['literal', $match[4]],
                ($match[5] ?? '') !== '' => ['punctuation', $match[5]],
                default => ['plain', $match[6] ?? ''],
            };
        }

        return $tokens;
    }
}
