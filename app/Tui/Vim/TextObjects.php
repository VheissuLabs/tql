<?php

namespace App\Tui\Vim;

final class TextObjects
{
    private const PAIRS = ['(' => ')', '[' => ']', '{' => '}'];

    public static function word(Text $text, int $at, bool $around, bool $big): Range
    {
        $lineStart = $text->lineStart($at);
        $lineEnd = $text->lineEnd($at);

        if ($lineStart === $lineEnd) {
            return new Range($at, $at);
        }

        $at = min($at, $lineEnd - 1);
        $kind = Motions::kind($text->at($at), $big);

        $start = self::extendBack($text, $at, $lineStart, $kind, $big);
        $end = self::extendForward($text, $at + 1, $lineEnd, $kind, $big);

        if (! $around) {
            return new Range($start, $end);
        }

        if ($kind === 0) {
            return new Range($start, self::extendForward($text, $end, $lineEnd, Motions::kind($text->at($end), $big), $big));
        }

        $trailing = self::extendForward($text, $end, $lineEnd, 0, $big);

        if ($trailing > $end) {
            return new Range($start, $trailing);
        }

        return new Range(self::extendBack($text, $start, $lineStart, 0, $big), $end);
    }

    public static function bracket(Text $text, int $at, string $open, bool $around): ?Range
    {
        $close = self::PAIRS[$open];

        $opener = $text->at($at) === $open
            ? $at
            : Motions::openerFor($text, $at, $open, $close);

        if ($opener === null) {
            return null;
        }

        $closer = Motions::closerFor($text, $opener, $open, $close);

        if ($closer === null) {
            return null;
        }

        return $around
            ? new Range($opener, $closer + 1)
            : new Range($opener + 1, $closer);
    }

    public static function quote(Text $text, int $at, string $quote, bool $around): ?Range
    {
        $lineStart = $text->lineStart($at);
        $lineEnd = $text->lineEnd($at);

        $marks = [];

        for ($offset = $lineStart; $offset < $lineEnd; $offset++) {
            if ($text->at($offset) === $quote) {
                $marks[] = $offset;
            }
        }

        $pair = self::quotePairFor($marks, $at);

        if ($pair === null) {
            return null;
        }

        [$opener, $closer] = $pair;

        if (! $around) {
            return new Range($opener + 1, $closer);
        }

        $end = $closer + 1;

        while ($end < $lineEnd && in_array($text->at($end), [' ', "\t"], true)) {
            $end++;
        }

        if ($end > $closer + 1) {
            return new Range($opener, $end);
        }

        $start = $opener;

        while ($start > $lineStart && in_array($text->at($start - 1), [' ', "\t"], true)) {
            $start--;
        }

        return new Range($start, $end);
    }

    public static function paragraph(Text $text, int $at, bool $around): Range
    {
        $line = $text->lineOf($at);
        $last = $text->lineCount() - 1;
        $blank = Motions::blank($text, $line);

        $first = $line;
        $final = $line;

        while ($first > 0 && Motions::blank($text, $first - 1) === $blank) {
            $first--;
        }

        while ($final < $last && Motions::blank($text, $final + 1) === $blank) {
            $final++;
        }

        if ($around) {
            $before = $final;

            while ($final < $last && Motions::blank($text, $final + 1) !== $blank) {
                $final++;
            }

            while ($final === $before && $first > 0 && Motions::blank($text, $first - 1) !== $blank) {
                $first--;
            }
        }

        return new Range(
            $text->startOfLine($first),
            $text->lineEnd($text->startOfLine($final)),
            linewise: true,
        );
    }

    private static function extendBack(Text $text, int $at, int $limit, int $kind, bool $big): int
    {
        while ($at > $limit && Motions::kind($text->at($at - 1), $big) === $kind) {
            $at--;
        }

        return $at;
    }

    private static function extendForward(Text $text, int $at, int $limit, int $kind, bool $big): int
    {
        while ($at < $limit && Motions::kind($text->at($at), $big) === $kind) {
            $at++;
        }

        return $at;
    }

    private static function quotePairFor(array $marks, int $at): ?array
    {
        $pairs = array_chunk($marks, 2);

        foreach ($pairs as $pair) {
            if (count($pair) === 2 && $at >= $pair[0] && $at <= $pair[1]) {
                return $pair;
            }
        }

        foreach ($pairs as $pair) {
            if (count($pair) === 2 && $pair[0] > $at) {
                return $pair;
            }
        }

        return null;
    }
}
