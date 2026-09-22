<?php

namespace App\Database;

/**
 * Rewrites the "order by" of a statement the user typed, so clicking a column
 * header sorts query results the same way it sorts a browsed table — and the
 * SQL pane shows them the clause that did it.
 */
class OrderBy
{
    public static function apply(string $statement, ?string $column, string $direction, string $wrapped): ?string
    {
        $statement = rtrim(trim($statement), ';');

        if (! static::rewritable($statement)) {
            return null;
        }

        $statement = static::strip($statement);

        if ($column === null) {
            return $statement;
        }

        $clause = 'order by '.$wrapped.' '.($direction === 'desc' ? 'desc' : 'asc');

        return preg_match('/\s(limit|offset)\s/i', $statement, $match, PREG_OFFSET_CAPTURE) === 1
            ? rtrim(substr($statement, 0, $match[0][1])).' '.$clause.' '.ltrim(substr($statement, $match[0][1]))
            : $statement.' '.$clause;
    }

    /**
     * The column and direction a statement orders by, so the header can show
     * what the query is really doing rather than what was last clicked.
     *
     * @return array{0: string, 1: string}|null
     */
    public static function of(string $statement): ?array
    {
        $matched = preg_match(
            '/\border\s+by\s+[`"\[]?([A-Za-z0-9_]+)[`"\]]?(?:\s+(asc|desc))?/i',
            $statement,
            $match,
        );

        if ($matched !== 1) {
            return null;
        }

        return [$match[1], strtolower($match[2] ?? 'asc')];
    }

    /**
     * Only a single plain select. A union or a second statement would need us
     * to work out which select the clause belongs to, and guessing wrong
     * rewrites the user's query into something they did not ask for.
     */
    private static function rewritable(string $statement): bool
    {
        return preg_match('/^\s*select\s/i', $statement) === 1
            && preg_match('/\bunion\b/i', $statement) !== 1
            && ! str_contains($statement, ';')
            && preg_match_all('/\border\s+by\b/i', $statement) <= 1;
    }

    private static function strip(string $statement): string
    {
        $stripped = preg_replace(
            '/\border\s+by\b.*?(?=\s(?:limit|offset)\s|$)/is',
            '',
            $statement,
        );

        return trim(preg_replace('/\s{2,}/', ' ', $stripped));
    }
}
