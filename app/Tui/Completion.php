<?php

namespace App\Tui;

use Closure;

class Completion
{
    public const LIMIT = 50;

    private const KEYWORDS_BEFORE_A_TABLE = ['from', 'join', 'into', 'update', 'table'];

    private const CLOSEST_SCORE_SHOWN = 2;

    private const IDENTIFIER = '(?:"[^"]+"|`[^`]+`|\[[^\]]+\]|[A-Za-z_][\w$]*)';

    public int $index = 0;

    public function __construct(public array $items, public int $start, public int $end) {}

    public static function at(string $buffer, int $cursor, array $tables, Closure $columns, ?string $open, Closure $quote): ?self
    {
        $before = mb_substr($buffer, 0, $cursor);

        if (self::inStringOrComment(self::lastLine($before))) {
            return null;
        }

        preg_match('/[\w$]*\z/u', $before, $word);
        $prefix = $word[0];
        $start = $cursor - mb_strlen($prefix);
        $ahead = mb_substr($before, 0, $start);

        if (preg_match('/^\d/', $prefix) === 1) {
            return null;
        }

        $referenced = self::referenced(Statements::at($buffer, $cursor)['sql'] ?? $buffer, $tables);

        if (preg_match('/('.self::IDENTIFIER.')\.\z/u', $ahead, $qualified) === 1) {
            return self::narrowed(
                self::columnsOfQualifier(self::unquote($qualified[1]), $referenced, $tables, $columns, $quote),
                $prefix,
                $start,
                $cursor,
                allowEmpty: true,
            );
        }

        if ($prefix === '') {
            return null;
        }

        preg_match('/([\w$]+)\W*\z/u', $ahead, $previous);
        $previousWord = mb_strtolower($previous[1] ?? '');

        if ($previousWord === 'as') {
            return null;
        }

        if (in_array($previousWord, self::KEYWORDS_BEFORE_A_TABLE, true)) {
            return self::narrowed(self::named($tables, 'table', $quote), $prefix, $start, $cursor);
        }

        return self::narrowed([
            ...self::named(self::columnsOf(array_values(array_unique($referenced)) ?: array_filter([$open]), $columns), 'column', $quote),
            ...self::named($tables, 'table', $quote),
            ...self::keywords($prefix),
        ], $prefix, $start, $cursor);
    }

    public function move(int $by): void
    {
        $count = count($this->items);

        $this->index = $count === 0
            ? 0
            : ($this->index + $by + $count) % $count;
    }

    public function selected(): ?array
    {
        return $this->items[$this->index] ?? null;
    }

    private static function columnsOfQualifier(string $qualifier, array $referenced, array $tables, Closure $columns, Closure $quote): array
    {
        $table = $referenced[mb_strtolower($qualifier)] ?? self::known($qualifier, $tables);

        if ($table === null) {
            return [];
        }

        return self::named($columns($table), 'column', $quote);
    }

    private static function columnsOf(array $tables, Closure $columns): array
    {
        $names = [];

        foreach ($tables as $table) {
            array_push($names, ...$columns($table));
        }

        return array_values(array_unique($names));
    }

    private static function narrowed(array $candidates, string $prefix, int $start, int $end, bool $allowEmpty = false): ?self
    {
        $needle = mb_strtolower($prefix);

        if ($needle === '' && ! $allowEmpty) {
            return null;
        }

        $ranked = [];

        foreach ($candidates as $order => $candidate) {
            $label = mb_strtolower($candidate['label']);

            if ($needle !== '' && $label === $needle) {
                continue;
            }

            $score = $needle === ''
                ? 1
                : Palette::score($label, $needle);

            if ($score !== null && $score <= self::CLOSEST_SCORE_SHOWN) {
                $ranked[] = [$score, $order, $candidate];
            }
        }

        if ($ranked === []) {
            return null;
        }

        usort($ranked, fn (array $first, array $second) => [$first[0], $first[1]] <=> [$second[0], $second[1]]);

        return new self(array_slice(array_column($ranked, 2), 0, self::LIMIT), $start, $end);
    }

    private static function referenced(string $sql, array $tables): array
    {
        $identifier = self::IDENTIFIER;
        $found = [];

        preg_match_all(
            "/\\b(?:from|join|update|into)\\s+((?:{$identifier}\\.)?{$identifier})(?:\\s+(?:as\\s+)?({$identifier}))?/iu",
            $sql,
            $matches,
            PREG_SET_ORDER,
        );

        foreach ($matches as $match) {
            $parts = explode('.', $match[1]);
            $table = self::known(self::unquote(end($parts)), $tables);

            if ($table === null) {
                continue;
            }

            $found[mb_strtolower($table)] = $table;

            $alias = self::unquote($match[2] ?? '');

            if ($alias !== '' && ! in_array(mb_strtolower($alias), Sql::KEYWORDS, true)) {
                $found[mb_strtolower($alias)] = $table;
            }
        }

        return $found;
    }

    private static function known(string $name, array $tables): ?string
    {
        foreach ($tables as $table) {
            if (mb_strtolower($table) === mb_strtolower($name)) {
                return $table;
            }
        }

        return null;
    }

    private static function named(array $names, string $kind, Closure $quote): array
    {
        return array_map(fn (string $name) => [
            'text' => self::needsQuoting($name)
                ? $quote($name)
                : $name,
            'label' => $name,
            'kind' => $kind,
        ], $names);
    }

    private static function needsQuoting(string $name): bool
    {
        return preg_match('/^[a-z_][a-z0-9_]*$/', $name) !== 1
            || in_array($name, Sql::KEYWORDS, true);
    }

    private static function keywords(string $prefix): array
    {
        $shouting = preg_match('/[A-Z]/', $prefix) === 1 && mb_strtoupper($prefix) === $prefix;
        $keywords = array_values(array_unique(Sql::KEYWORDS));

        sort($keywords);

        return array_map(fn (string $keyword) => [
            'text' => $shouting
                ? mb_strtoupper($keyword)
                : $keyword,
            'label' => $keyword,
            'kind' => 'keyword',
        ], $keywords);
    }

    private static function unquote(string $name): string
    {
        return preg_match('/^(["`\[])(.*)["`\]]$/s', $name, $match) === 1
            ? $match[2]
            : $name;
    }

    private static function lastLine(string $text): string
    {
        return mb_substr($text, (int) mb_strrpos("\n".$text, "\n"));
    }

    private static function inStringOrComment(string $line): bool
    {
        $outsideQuotes = (string) preg_replace('/\'(?:\'\'|[^\'])*\'|"[^"]*"|`[^`]*`/', '', $line);

        return str_contains($outsideQuotes, "'")
            || str_contains($outsideQuotes, '"')
            || str_contains($outsideQuotes, '`')
            || str_contains($outsideQuotes, '--');
    }
}
