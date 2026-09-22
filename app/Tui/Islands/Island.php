<?php

namespace App\Tui\Islands;

abstract class Island
{
    public int $x = 1;

    public int $y = 1;

    public int $width = 10;

    public int $height = 3;

    public bool $focused = false;

    public string $title = '';

    abstract public function content(int $innerWidth, int $innerHeight): array;

    public function place(int $x, int $y, int $width, int $height): static
    {
        $this->x = $x;
        $this->y = $y;
        $this->width = max(3, $width);
        $this->height = max(3, $height);

        return $this;
    }

    public function innerWidth(): int
    {
        return $this->width - 2;
    }

    public function innerHeight(): int
    {
        return $this->height - 2;
    }

    public function contains(int $column, int $row): bool
    {
        return $column >= $this->x
            && $column <= $this->x + $this->width - 1
            && $row >= $this->y
            && $row <= $this->y + $this->height - 1;
    }

    public function containsContent(int $column, int $row): bool
    {
        return $this->contains($column, $row)
            && $this->localRow($row) >= 0
            && $this->localRow($row) < $this->innerHeight()
            && $this->localColumn($column) >= 0
            && $this->localColumn($column) < $this->innerWidth();
    }

    public function localRow(int $row): int
    {
        return $row - $this->y - 1;
    }

    public function localColumn(int $column): int
    {
        return $column - $this->x - 1;
    }

    /** Drawn as a title bar only, with no body. */
    public bool $collapsed = false;

    /** Drawn with no border at all: a backdrop behind a modal. */
    public bool $bare = false;

    /** Floating over the panes, so it takes the modal colours. */
    public bool $modal = false;

    public function joins(): array
    {
        return [];
    }

    public function ruleRows(): array
    {
        return [];
    }

    public function contentColumn(int $local): int
    {
        return $this->x + 1 + $local;
    }

    /**
     * Where to start drawing so the cursor stays in view, centred when there
     * is more than fits.
     */
    protected function window(int $cursor, int $total, int $room): int
    {
        if ($total <= $room) {
            return 0;
        }

        return max(0, min($cursor - intdiv($room, 2), $total - $room));
    }
}
