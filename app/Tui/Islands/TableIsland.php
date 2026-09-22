<?php

namespace App\Tui\Islands;

use App\Tui\Layout;

class TableIsland extends Island
{
    public string $title = 'ROWS';

    public int $rowStart = 0;

    public int $columnOffset = 0;

    public bool $scrollLocked = false;

    private array $widths = [];

    public function __construct(
        private array $headers,
        private array $rows,
        private int $rowIndex,
        private int $columnIndex,
        private array $overrides,
        private ?string $editing,
        private Styler $style,
        private ?string $sortColumn = null,
        private string $sortDirection = 'asc',
        private array $marked = [],
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        if ($this->headers === []) {
            return [$this->style->dim('no columns')];
        }

        $this->widths = $this->fit($innerWidth - self::GUTTER);

        $lines = [
            str_repeat(' ', self::GUTTER).$this->headerLine(),
            $this->rule($innerWidth),
        ];

        $room = max(1, $innerHeight - 2);
        $this->rowStart = $this->window($this->rowIndex, count($this->rows), $room);

        foreach (array_slice($this->rows, $this->rowStart, $room) as $index => $row) {
            $lines[] = $this->rowLine($row, $this->rowStart + $index, $innerWidth);
        }

        $blank = str_repeat(' ', self::GUTTER).$this->blankLine();

        while (count($lines) < $innerHeight) {
            $lines[] = $blank;
        }

        return $lines;
    }

    public function ruleRows(): array
    {
        return $this->headers === [] ? [] : [1];
    }

    public function joins(): array
    {
        $joins = [];
        $x = self::GUTTER;

        foreach ($this->widths as $i => $width) {
            if ($i === count($this->widths) - 1) {
                break;
            }

            $x += $width + 2;
            $joins[] = $x;
            $x += 1;
        }

        return $joins;
    }

    public function handles(): array
    {
        $handles = [];
        $x = $this->contentColumn(self::GUTTER);

        foreach ($this->widths as $i => $width) {
            $x += $width + 2;
            $handles[$this->columnOffset + $i] = $x;
            $x += 1;
        }

        return $handles;
    }

    public function cellStart(int $absoluteColumn): ?int
    {
        $x = $this->contentColumn(self::GUTTER);

        foreach ($this->widths as $i => $width) {
            if ($this->columnOffset + $i === $absoluteColumn) {
                return $x;
            }

            $x += $width + 3;
        }

        return null;
    }

    public function widthOf(int $absoluteColumn): ?int
    {
        $index = $absoluteColumn - $this->columnOffset;

        return $this->widths[$index] ?? null;
    }

    public function rowIndexFor(int $localRow): ?int
    {
        $target = $this->rowStart + $localRow - 2;

        return $target >= 0 && $target < count($this->rows) ? $target : null;
    }

    public function columnIndexFor(int $localColumn): ?int
    {
        $localColumn -= self::GUTTER;
        $offset = 0;

        foreach ($this->widths as $i => $width) {
            if ($localColumn >= $offset && $localColumn < $offset + $width + 2) {
                return $this->columnOffset + $i;
            }

            $offset += $width + 3;
        }

        return null;
    }

    public const COMFORTABLE = 28;

    public const GUTTER = 2;

    public function naturalWidth(string $column): int
    {
        // The sort marker lives in the header, so the column has to be wide
        // enough to hold it or it is the first thing truncation eats.
        $width = mb_strlen($column) + ($column === $this->sortColumn ? 2 : 0);

        foreach ($this->rows as $row) {
            $width = max($width, mb_strlen((string) ($row[$column] ?? '')));
        }

        return $width;
    }

    private function headerLine(): string
    {
        $cells = [];

        foreach (array_slice($this->headers, $this->columnOffset, count($this->widths)) as $i => $name) {
            $label = $name === $this->sortColumn
                ? $name.' '.($this->sortDirection === 'desc' ? '▼' : '▲')
                : $name;

            $text = ' '.$this->style->pad($this->style->truncate($label, $this->widths[$i]), $this->widths[$i]).' ';

            $cells[] = ($this->columnOffset + $i) === $this->columnIndex
                ? $this->style->bold($text)
                : $this->style->dim($text);
        }

        return implode($this->style->colour('grid', '│'), $cells);
    }

    private function rule(int $innerWidth): string
    {
        $segments = array_map(fn (int $w) => str_repeat('─', $w + 2), $this->widths);

        $body = str_repeat('─', self::GUTTER).implode('┼', $segments);

        return $this->style->colour('grid', $body.str_repeat('─', max(0, $innerWidth - mb_strlen($body))));
    }

    private function blankLine(): string
    {
        $cells = array_map(fn (int $w) => str_repeat(' ', $w + 2), $this->widths);

        return implode($this->style->colour('grid', '│'), $cells);
    }

    private function rowLine(array $row, int $absolute, int $innerWidth): string
    {
        $selected = $absolute === $this->rowIndex;
        $pending = in_array($absolute, $this->marked, true);
        $values = array_values($row);
        $cells = [];
        $style = Layout::rowStyle();

        foreach ($this->widths as $i => $width) {
            $column = $this->columnOffset + $i;
            $value = (string) ($values[$column] ?? '');

            $text = $this->editing !== null && $selected && $column === $this->columnIndex
                ? $this->editBuffer($this->editing, $width)
                : $this->style->truncate($value, $width);

            $padded = ' '.$this->style->pad($text, $width).' ';

            $cells[] = $selected && $column === $this->columnIndex && ! $pending
                ? $this->cursorCell($text, $width)
                : $padded;
        }

        // A marked row is drawn as one bar, so it is built without colour of
        // its own: an escape sequence inside the span would tear the
        // highlight at the first column separator.
        if ($pending) {
            $marker = $selected && $style === 'marker' ? ' ▸' : '  ';

            return $this->style->colour(
                'marked',
                $this->style->pad($marker.implode('│', $cells), $innerWidth),
            );
        }

        $marker = $selected && $style === 'marker' ? ' ▸' : '  ';

        $line = $marker.implode($this->style->colour('grid', '│'), $cells);

        if (! $selected) {
            return $style === 'dim-others' ? $this->style->dim($line) : $line;
        }

        if ($this->editing !== null) {
            return $line;
        }

        return match ($style) {
            'underline' => $this->style->underline($this->style->pad($line, $innerWidth)),
            'inverse' => $this->style->colour('selection', $this->style->pad($line, $innerWidth)),
            'bold' => $this->style->bold($line),
            default => $line,
        };
    }

    /**
     * Highlight the value, not the column. Padding the block out to the full
     * width makes the cursor look far bigger than the thing it is on.
     */
    private function cursorCell(string $text, int $width): string
    {
        $visible = max(1, $this->style->visible($text));

        // A column of padding either side, so the block frames the value
        // rather than sitting tight against it.
        return $this->style->colour('cursor', ' '.($text === '' ? ' ' : $text).' ')
            .str_repeat(' ', max(0, $width - $visible));
    }

    private function editBuffer(string $buffer, int $width): string
    {
        $text = $buffer.'█';

        return mb_strlen($text) > $width ? mb_substr($text, -$width) : $text;
    }

    private function fit(int $available): array
    {
        $all = $this->desiredWidths($available, false);

        if ($this->total($all) > $available) {
            $all = $this->desiredWidths($available, true);
        }

        $this->columnOffset = $this->scroll($all, $available);

        $widths = [];
        $used = 0;

        for ($i = $this->columnOffset; $i < count($all); $i++) {
            $cost = $all[$i] + 2 + ($widths === [] ? 0 : 1);

            $reachedCursor = ($this->columnOffset + count($widths) - 1) >= $this->columnIndex;

            if ($used + $cost > $available && $widths !== [] && $reachedCursor) {
                break;
            }

            $widths[] = $all[$i];
            $used += $cost;
        }

        return $this->grow($widths, $available);
    }

    private function grow(array $widths, int $available): array
    {
        $leftover = $available - $this->total($widths);

        if ($leftover <= 0 || $widths === []) {
            return $widths;
        }

        if ($this->scrollLocked) {
            return $this->growLast($widths, $leftover);
        }

        $widths = $this->shareOut($widths, $leftover);

        $leftover = $available - $this->total($widths);

        return $leftover > 0 ? $this->growLast($widths, $leftover) : $widths;
    }

    private function shareOut(array $widths, int $leftover): array
    {
        $wanted = [];

        foreach ($widths as $i => $width) {
            $name = $this->headers[$this->columnOffset + $i] ?? null;

            if ($name === null || isset($this->overrides[$name])) {
                continue;
            }

            $deficit = $this->naturalWidth($name) - $width;

            if ($deficit > 0) {
                $wanted[$i] = $deficit;
            }
        }

        while ($leftover > 0 && $wanted !== []) {
            foreach ($wanted as $i => $deficit) {
                if ($leftover === 0) {
                    break;
                }

                $widths[$i]++;
                $leftover--;

                if (--$wanted[$i] === 0) {
                    unset($wanted[$i]);
                }
            }
        }

        return $widths;
    }

    private function growLast(array $widths, int $leftover): array
    {
        $last = count($widths) - 1;
        $name = $this->headers[$this->columnOffset + $last] ?? null;

        if ($name === null || isset($this->overrides[$name])) {
            return $widths;
        }

        $widths[$last] += $leftover;

        return $widths;
    }

    private function desiredWidths(int $available, bool $capped): array
    {
        $widths = [];

        foreach ($this->headers as $index => $name) {
            $width = $this->overrides[$name] ?? $this->naturalWidth($name);

            if ($capped && ! isset($this->overrides[$name])) {
                $width = min($width, self::COMFORTABLE);
            }

            if ($this->editing !== null && $index === $this->columnIndex) {
                $width = max($width, min(24, $available - 3));
            }

            $widths[] = max(3, min($width, $available - 3));
        }

        return $widths;
    }

    private function total(array $widths): int
    {
        if ($widths === []) {
            return 0;
        }

        return array_sum(array_map(fn (int $w) => $w + 2, $widths)) + count($widths) - 1;
    }

    private function scroll(array $all, int $available): int
    {
        if ($this->scrollLocked) {
            return $this->columnOffset;
        }

        $offset = min($this->columnOffset, $this->columnIndex);

        while ($offset < $this->columnIndex) {
            $used = 0;

            for ($i = $offset; $i <= $this->columnIndex; $i++) {
                $used += ($all[$i] ?? 0) + 3;
            }

            if ($used <= $available) {
                break;
            }

            $offset++;
        }

        return $offset;
    }

    private function window(int $cursor, int $total, int $room): int
    {
        if ($total <= $room) {
            return 0;
        }

        return max(0, min($cursor - intdiv($room, 2), $total - $room));
    }
}
