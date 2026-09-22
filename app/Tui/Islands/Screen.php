<?php

namespace App\Tui\Islands;

class Screen
{
    private array $islands = [];

    public function add(Island ...$islands): static
    {
        foreach ($islands as $island) {
            $this->islands[] = $island;
        }

        return $this;
    }

    public function islands(): array
    {
        return $this->islands;
    }

    public function compose(int $height, callable $box): array
    {
        $rendered = [];

        foreach ($this->islands as $index => $island) {
            $rendered[$index] = $box($island);
        }

        $lines = [];

        for ($row = 1; $row <= $height; $row++) {
            $line = '';
            $cursor = 1;

            foreach ($this->sortedByColumn() as $index => $island) {
                if ($row < $island->y || $row > $island->y + $island->height - 1) {
                    continue;
                }

                if ($island->x > $cursor) {
                    $line .= str_repeat(' ', $island->x - $cursor);
                    $cursor = $island->x;
                }

                $line .= $rendered[$index][$row - $island->y] ?? '';
                $cursor += $island->width;
            }

            $lines[] = $line;
        }

        return $lines;
    }

    private function sortedByColumn(): array
    {
        $islands = $this->islands;

        uasort($islands, fn (Island $a, Island $b) => $a->x <=> $b->x);

        return $islands;
    }
}
