<?php

namespace App\Tui\Islands;

class SidebarIsland extends Island
{
    public string $title = 'TABLES';

    public int $start = 0;

    public function __construct(
        private array $tables,
        private int $selected,
        private Styler $style,
        private ?string $filter = null,
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $this->start = $this->window($this->selected, count($this->tables), $innerHeight);

        $lines = [];

        if ($this->tables === []) {
            return [$this->style->dim(' nothing matches')];
        }

        foreach (array_slice($this->tables, $this->start, $innerHeight) as $index => $table) {
            $label = ' '.$this->style->truncate($table, $innerWidth - 2);

            $lines[] = ($this->start + $index) === $this->selected
                ? $this->style->colour('selection', $this->style->pad($label, $innerWidth))
                : $this->style->dim($label);
        }

        return $lines;
    }

    public function heading(): string
    {
        return $this->filter === null || $this->filter === ''
            ? 'TABLES'
            : 'TABLES /'.$this->filter;
    }

    public function selectedIndexFor(int $localRow): ?int
    {
        $target = $this->start + $localRow;

        return $target >= 0 && $target < count($this->tables) ? $target : null;
    }

    public function isSelected(int $localRow): bool
    {
        return $this->start + $localRow === $this->selected;
    }
}
