<?php

namespace App\Tui\Islands;

class Screen
{
    private array $islands = [];

    /** Islands drawn on top of the composed screen rather than beside it. */
    private array $overlays = [];

    public function add(Island ...$islands): static
    {
        foreach ($islands as $island) {
            $this->islands[] = $island;
        }

        return $this;
    }

    /**
     * A modal: drawn over the panes instead of taking a share of the row, so
     * what it is covering stays visible around it.
     */
    public function overlay(Island $island): static
    {
        $this->overlays[] = $island;

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

        // Each overlay is drawn over the last, so a picker can sit on top of
        // the form that opened it. Note the separate variable: reusing $box
        // would replace the callable with the first overlay's lines.
        foreach ($this->overlays as $island) {
            $rows = $box($island);

            for ($i = 0; $i < $island->height; $i++) {
                $row = $island->y + $i - 1;

                if (! isset($lines[$row]) || ! isset($rows[$i])) {
                    continue;
                }

                $lines[$row] = static::splice($lines[$row], $rows[$i], $island->x, $island->width);
            }
        }

        return $lines;
    }

    /**
     * Drop $width visible columns from $line starting at column $x and put
     * $patch there, keeping the styling on either side intact.
     */
    public static function splice(string $line, string $patch, int $x, int $width): string
    {
        // Clip the patch too: an overlay that draws wider than its own rect
        // would push the rest of the row off the screen.
        return static::cut($line, 0, $x - 1)
            .static::cut($patch, 0, $width)
            ."\e[0m"
            .static::cut($line, $x - 1 + $width, PHP_INT_MAX);
    }

    /**
     * Take $length visible columns from $offset, carrying every escape
     * sequence along so colors set earlier in the line still apply.
     */
    private static function cut(string $line, int $offset, int $length): string
    {
        $out = '';
        $visible = 0;

        foreach (static::tokens($line) as $token) {
            if ($token[0] === "\e") {
                $out .= $token;

                continue;
            }

            if ($visible >= $offset && $visible < $offset + $length) {
                $out .= $token;
            }

            $visible++;
        }

        return $out;
    }

    /**
     * @return array<int, string>
     */
    private static function tokens(string $line): array
    {
        preg_match_all('/\e\[[0-9;]*m|./u', $line, $matches);

        return $matches[0];
    }

    private function sortedByColumn(): array
    {
        $islands = $this->islands;

        uasort($islands, fn (Island $a, Island $b) => $a->x <=> $b->x);

        return $islands;
    }
}
