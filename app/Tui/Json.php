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
        $pattern = '/("(?:\\\\.|[^"\\\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([\[\]{},:])/';

        $tokens = [];
        $offset = 0;

        if (preg_match_all($pattern, $line, $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE)) {
            foreach ($matches as $match) {
                $start = $match[0][1];

                if ($start > $offset) {
                    $tokens[] = ['plain', substr($line, $offset, $start - $offset)];
                }

                $tokens = array_merge($tokens, static::classify($match));

                $offset = $start + strlen($match[0][0]);
            }
        }

        if ($offset < strlen($line)) {
            $tokens[] = ['plain', substr($line, $offset)];
        }

        return $tokens;
    }

    private static function classify(array $match): array
    {
        $value = fn (int $group) => ($match[$group][0] ?? '') !== '' ? $match[$group][0] : null;

        if ($string = $value(1)) {
            $colon = $value(2);

            return $colon === null
                ? [['string', $string]]
                : [['key', $string], ['punctuation', $colon]];
        }

        return match (true) {
            ($number = $value(3)) !== null => [['number', $number]],
            ($literal = $value(4)) !== null => [['literal', $literal]],
            ($punctuation = $value(5)) !== null => [['punctuation', $punctuation]],
            default => [['plain', $match[0][0]]],
        };
    }
}
