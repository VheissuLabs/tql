<?php

namespace App\Database;

use Illuminate\Database\Query\Grammars\Grammar;

/**
 * One condition in the filter bar: a column, an operator, and a value.
 *
 * The clause is built with a placeholder and the value is bound, never
 * interpolated, so a value containing a quote is a value rather than SQL.
 */
class Filter
{
    public const OPERATORS = [
        'is' => '=',
        'is not' => '!=',
        'contains' => 'like',
        'starts with' => 'like',
        'ends with' => 'like',
        'is greater than' => '>',
        'is at least' => '>=',
        'is less than' => '<',
        'is at most' => '<=',
        'is empty' => 'is null',
        'is not empty' => 'is not null',
        'is one of' => 'in',
    ];

    public function __construct(
        public string $column,
        public string $operator = 'is',
        public string $value = '',
    ) {}

    public function needsValue(): bool
    {
        return ! in_array($this->operator, ['is empty', 'is not empty'], true);
    }

    public function usable(): bool
    {
        return $this->column !== '' && (! $this->needsValue() || $this->value !== '');
    }

    /**
     * @return array{0: string, 1: array<int, mixed>}|null
     */
    public function toSql(Grammar $grammar): ?array
    {
        if (! $this->usable()) {
            return null;
        }

        $column = $grammar->wrap($this->column);

        return match ($this->operator) {
            'is empty' => ["{$column} is null", []],
            'is not empty' => ["{$column} is not null", []],
            'contains' => ["{$column} like ?", ['%'.$this->value.'%']],
            'starts with' => ["{$column} like ?", [$this->value.'%']],
            'ends with' => ["{$column} like ?", ['%'.$this->value]],
            'is one of' => $this->oneOf($column),
            default => ["{$column} ".self::OPERATORS[$this->operator].' ?', [$this->value]],
        };
    }

    /**
     * @return array{0: string, 1: array<int, mixed>}
     */
    private function oneOf(string $column): array
    {
        $values = array_values(array_filter(
            array_map('trim', explode(',', $this->value)),
            fn (string $value) => $value !== '',
        ));

        if ($values === []) {
            return ['1 = 1', []];
        }

        return [
            $column.' in ('.implode(', ', array_fill(0, count($values), '?')).')',
            $values,
        ];
    }

    /**
     * How the condition reads in the bar, for the modal.
     */
    public function describe(): string
    {
        return trim($this->column.' '.$this->operator.' '.($this->needsValue() ? $this->value : ''));
    }
}
