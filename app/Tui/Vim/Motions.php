<?php

namespace App\Tui\Vim;

final class Motions
{
    private const PAIRS = ['(' => ')', '[' => ']', '{' => '}'];

    public static function kind(string $char, bool $big = false): int
    {
        if ($char === '' || trim($char) === '') {
            return 0;
        }

        if ($big) {
            return 1;
        }

        return preg_match('/^[\p{L}\p{N}_]$/u', $char) === 1
            ? 1
            : 2;
    }

    public static function left(Text $text, int $at, int $count): int
    {
        return max($text->lineStart($at), $at - $count);
    }

    public static function right(Text $text, int $at, int $count): int
    {
        return min($text->lineEnd($at), $at + $count);
    }

    public static function vertical(Text $text, int $at, int $by, int $column): ?int
    {
        $line = $text->lineOf($at);
        $target = max(0, min($text->lineCount() - 1, $line + $by));

        if ($target === $line) {
            return null;
        }

        $start = $text->startOfLine($target);

        return $start + min($column, max(0, $text->lineEnd($start) - $start - 1));
    }

    public static function lineStart(Text $text, int $at): int
    {
        return $text->lineStart($at);
    }

    public static function firstNonBlank(Text $text, int $at): int
    {
        $offset = $text->lineStart($at);
        $end = $text->lineEnd($offset);

        while ($offset < $end && in_array($text->at($offset), [' ', "\t"], true)) {
            $offset++;
        }

        return $offset;
    }

    public static function lastCharacter(Text $text, int $at, int $count): int
    {
        $line = min($text->lineCount() - 1, $text->lineOf($at) + $count - 1);
        $start = $text->startOfLine($line);
        $end = $text->lineEnd($start);

        return $end > $start
            ? $end - 1
            : $start;
    }

    public static function line(Text $text, int $line): int
    {
        return self::firstNonBlank($text, $text->startOfLine(max(0, min($line, $text->lineCount() - 1))));
    }

    public static function wordForward(Text $text, int $at, int $count, bool $big): int
    {
        for ($step = 0; $step < $count; $step++) {
            $at = self::nextWordStart($text, $at, $big);
        }

        return $at;
    }

    public static function wordBackward(Text $text, int $at, int $count, bool $big): int
    {
        for ($step = 0; $step < $count; $step++) {
            $at = self::previousWordStart($text, $at, $big);
        }

        return $at;
    }

    public static function wordEnd(Text $text, int $at, int $count, bool $big): int
    {
        for ($step = 0; $step < $count; $step++) {
            $at = self::nextWordEnd($text, $at, $big);
        }

        return $at;
    }

    public static function findInLine(Text $text, int $at, string $char, int $count, bool $forward, bool $till): ?int
    {
        $found = 0;

        if ($forward) {
            for ($offset = $at + 1, $end = $text->lineEnd($at); $offset < $end; $offset++) {
                if ($text->at($offset) === $char && ++$found === $count) {
                    return $till
                        ? $offset - 1
                        : $offset;
                }
            }

            return null;
        }

        for ($offset = $at - 1, $start = $text->lineStart($at); $offset >= $start; $offset--) {
            if ($text->at($offset) === $char && ++$found === $count) {
                return $till
                    ? $offset + 1
                    : $offset;
            }
        }

        return null;
    }

    public static function matchingBracket(Text $text, int $at): ?int
    {
        $end = $text->lineEnd($at);
        $closers = array_flip(self::PAIRS);

        for ($offset = $at; $offset < $end; $offset++) {
            $char = $text->at($offset);

            if (isset(self::PAIRS[$char])) {
                return self::closerFor($text, $offset, $char, self::PAIRS[$char]);
            }

            if (isset($closers[$char])) {
                return self::openerFor($text, $offset, $closers[$char], $char);
            }
        }

        return null;
    }

    public static function closerFor(Text $text, int $opener, string $open, string $close): ?int
    {
        $depth = 0;

        for ($offset = $opener + 1; $offset < $text->length; $offset++) {
            $char = $text->at($offset);

            if ($char === $open) {
                $depth++;
            }

            if ($char === $close && $depth-- === 0) {
                return $offset;
            }
        }

        return null;
    }

    public static function openerFor(Text $text, int $closer, string $open, string $close): ?int
    {
        $depth = 0;

        for ($offset = $closer - 1; $offset >= 0; $offset--) {
            $char = $text->at($offset);

            if ($char === $close) {
                $depth++;
            }

            if ($char === $open && $depth-- === 0) {
                return $offset;
            }
        }

        return null;
    }

    public static function paragraphForward(Text $text, int $at, int $count): int
    {
        $line = $text->lineOf($at);
        $last = $text->lineCount() - 1;

        for ($step = 0; $step < $count; $step++) {
            while ($line < $last && self::blank($text, $line)) {
                $line++;
            }

            while ($line < $last && ! self::blank($text, $line)) {
                $line++;
            }
        }

        if ($line === $last && ! self::blank($text, $line)) {
            return max(0, $text->length - 1);
        }

        return $text->startOfLine($line);
    }

    public static function paragraphBackward(Text $text, int $at, int $count): int
    {
        $line = $text->lineOf($at);

        for ($step = 0; $step < $count; $step++) {
            while ($line > 0 && self::blank($text, $line)) {
                $line--;
            }

            while ($line > 0 && ! self::blank($text, $line)) {
                $line--;
            }
        }

        return $text->startOfLine($line);
    }

    public static function blank(Text $text, int $line): bool
    {
        return $text->isBlankLine($text->startOfLine($line));
    }

    private static function nextWordStart(Text $text, int $at, bool $big): int
    {
        if ($at >= $text->length) {
            return $text->length;
        }

        $kind = self::kind($text->at($at), $big);

        if ($kind !== 0) {
            while ($at < $text->length && self::kind($text->at($at), $big) === $kind) {
                $at++;
            }
        }

        while ($at < $text->length && self::kind($text->at($at), $big) === 0) {
            if ($text->at($at) === "\n" && $text->at($at + 1) === "\n") {
                return $at + 1;
            }

            $at++;
        }

        return $at;
    }

    private static function previousWordStart(Text $text, int $at, bool $big): int
    {
        if ($at <= 0) {
            return 0;
        }

        $at--;

        while ($at > 0 && self::kind($text->at($at), $big) === 0) {
            if ($text->at($at) === "\n" && $text->at($at - 1) === "\n") {
                return $at;
            }

            $at--;
        }

        $kind = self::kind($text->at($at), $big);

        while ($at > 0 && self::kind($text->at($at - 1), $big) === $kind) {
            $at--;
        }

        return $at;
    }

    private static function nextWordEnd(Text $text, int $at, bool $big): int
    {
        if ($at >= $text->length - 1) {
            return max(0, $text->length - 1);
        }

        $at++;

        while ($at < $text->length - 1 && self::kind($text->at($at), $big) === 0) {
            $at++;
        }

        $kind = self::kind($text->at($at), $big);

        while ($at + 1 < $text->length && self::kind($text->at($at + 1), $big) === $kind) {
            $at++;
        }

        return $at;
    }
}
