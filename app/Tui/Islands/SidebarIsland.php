<?php

namespace App\Tui\Islands;

class SidebarIsland extends Island
{
    public string $title = 'TABLES';

    public int $start = 0;

    public function __construct(
        private array $tables,
        private int $selected,
        private \Closure $truncate,
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $this->start = $this->window($this->selected, count($this->tables), $innerHeight);

        $lines = [];

        foreach (array_slice($this->tables, $this->start, $innerHeight) as $index => $table) {
            $lines[] = ' '.($this->truncate)($table, $innerWidth - 2);
        }

        return $lines;
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

    private function window(int $cursor, int $total, int $room): int
    {
        if ($total <= $room) {
            return 0;
        }

        return max(0, min($cursor - intdiv($room, 2), $total - $room));
    }
}
