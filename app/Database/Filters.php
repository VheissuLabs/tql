<?php

namespace App\Database;

use Illuminate\Database\Query\Grammars\Grammar;

class Filters
{
    /**
     * @param  array<int, Filter>  $filters
     */
    public function __construct(public array $filters = [], public string $joiner = 'and') {}

    /**
     * The where clause and its bindings, or null when nothing is usable yet.
     *
     * @return array{0: string, 1: array<int, mixed>}|null
     */
    public function toSql(Grammar $grammar): ?array
    {
        $clauses = [];
        $bindings = [];

        foreach ($this->filters as $filter) {
            $built = $filter->toSql($grammar);

            if ($built === null) {
                continue;
            }

            $clauses[] = $built[0];
            $bindings = array_merge($bindings, $built[1]);
        }

        if ($clauses === []) {
            return null;
        }

        $joiner = $this->joiner === 'or' ? ' or ' : ' and ';

        return [implode($joiner, $clauses), $bindings];
    }

    public function usable(): bool
    {
        foreach ($this->filters as $filter) {
            if ($filter->usable()) {
                return true;
            }
        }

        return false;
    }
}
