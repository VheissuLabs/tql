<?php

namespace App\Tui;

use App\Database\Filter;
use App\Database\Filters;

/**
 * The filter bar's state: a list of conditions, and where the cursor is in
 * them. Nothing runs until it is applied, so building a condition does not
 * fire a query per keystroke.
 */
class FilterForm
{
    public const COLUMN = 0;

    public const OPERATOR = 1;

    public const VALUE = 2;

    public int $row = 0;

    public int $cell = self::COLUMN;

    public ?QueryEditor $editor = null;

    /** The type-to-filter list for the column or operator cell. */
    public ?Picker $picker = null;

    public string $joiner = 'and';

    /** @var array<int, Filter> */
    public array $conditions = [];

    /**
     * @param  array<int, string>  $columns
     */
    public function __construct(
        public array $columns,
        ?Filters $existing = null,
        private ?string $startOn = null,
    ) {
        $this->conditions = $existing?->filters ?? [];
        $this->joiner = $existing?->joiner ?? 'and';

        if ($this->conditions === []) {
            $this->add();

            // Start on the column you were looking at, already typing: the
            // column and operator are guesses, and the value never is. Escape
            // steps back into the form to change them.
            $this->cell = self::VALUE;
            $this->startEditing();
        }
    }

    public function add(): void
    {
        $column = $this->startOn !== null && in_array($this->startOn, $this->columns, true)
            ? $this->startOn
            : ($this->columns[0] ?? '');

        $this->conditions[] = new Filter($column, 'contains', '');
        $this->row = count($this->conditions) - 1;
        $this->cell = self::COLUMN;
    }

    public function remove(): void
    {
        if (count($this->conditions) <= 1) {
            $this->conditions = [];
            $this->add();

            return;
        }

        unset($this->conditions[$this->row]);

        $this->conditions = array_values($this->conditions);
        $this->row = min($this->row, count($this->conditions) - 1);
    }

    public function current(): Filter
    {
        return $this->conditions[$this->row];
    }

    public function moveRow(int $by): void
    {
        $this->row = max(0, min(count($this->conditions) - 1, $this->row + $by));
        $this->cell = min($this->cell, $this->lastCell());
    }

    public function moveCell(int $by): void
    {
        $this->cell = max(self::COLUMN, min($this->lastCell(), $this->cell + $by));
    }

    /**
     * An operator that takes no value has no value cell to move to.
     */
    private function lastCell(): int
    {
        return $this->current()->needsValue() ? self::VALUE : self::OPERATOR;
    }

    public function cycle(int $by): void
    {
        $filter = $this->current();

        if ($this->cell === self::COLUMN) {
            $filter->column = $this->next($this->columns, $filter->column, $by);

            return;
        }

        if ($this->cell === self::OPERATOR) {
            $filter->operator = $this->next(array_keys(Filter::OPERATORS), $filter->operator, $by);
            $this->cell = min($this->cell, $this->lastCell());
        }
    }

    /**
     * @param  array<int, string>  $options
     */
    private function next(array $options, string $current, int $by): string
    {
        if ($options === []) {
            return $current;
        }

        $at = array_search($current, $options, true);
        $at = $at === false ? 0 : $at;

        return $options[($at + $by + count($options)) % count($options)];
    }

    public function toggleJoiner(): void
    {
        $this->joiner = $this->joiner === 'and' ? 'or' : 'and';
    }

    /**
     * Enter on the column or operator cell opens a list you can type into,
     * which beats cycling past forty columns one arrow at a time.
     */
    public function openPicker(): void
    {
        $filter = $this->current();

        $this->picker = match ($this->cell) {
            self::COLUMN => new Picker('COLUMN', $this->columns, $filter->column),
            self::OPERATOR => new Picker('OPERATOR', array_keys(Filter::OPERATORS), $filter->operator),
            default => null,
        };
    }

    public function choose(): void
    {
        $chosen = $this->picker?->selected();

        if ($chosen === null) {
            $this->picker = null;

            return;
        }

        if ($this->cell === self::COLUMN) {
            $this->current()->column = $chosen;
        } else {
            $this->current()->operator = $chosen;
            $this->cell = min($this->cell, $this->lastCell());
        }

        $this->picker = null;
    }

    public function closePicker(): void
    {
        $this->picker = null;
    }

    public function startEditing(): void
    {
        if ($this->cell !== self::VALUE) {
            return;
        }

        $this->editor = new QueryEditor(multiline: false);
        $this->editor->set($this->current()->value);
        $this->editor->toEnd();
    }

    public function commit(): void
    {
        if ($this->editor !== null) {
            $this->current()->value = $this->editor->buffer();
        }

        $this->editor = null;
    }

    public function abandon(): void
    {
        $this->editor = null;
    }

    public function toFilters(): Filters
    {
        return new Filters($this->conditions, $this->joiner);
    }
}
