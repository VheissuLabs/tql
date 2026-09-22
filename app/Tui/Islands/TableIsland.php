<?php

namespace App\Tui\Islands;

class TableIsland extends Island
{
    public string $title = 'ROWS';

    public int $rowStart = 0;

    public int $columnOffset = 0;

    private array $widths = [];

    public function __construct(
        private array $headers,
        private array $rows,
        private int $rowIndex,
        private int $columnIndex,
        private array $overrides,
        private ?string $editing,
        private Styler $style,
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        if ($this->headers === []) {
            return [$this->style->dim('no columns')];
        }

        $this->widths = $this->fit($innerWidth);

        $lines = [$this->headerLine(), $this->rule($innerWidth)];

        $room = max(1, $innerHeight - 2);
        $this->rowStart = $this->window($this->rowIndex, count($this->rows), $room);

        foreach (array_slice($this->rows, $this->rowStart, $room) as $index => $row) {
            $lines[] = $this->rowLine($row, $this->rowStart + $index, $innerWidth);
        }

        $blank = $this->blankLine();

        while (count($lines) < $innerHeight) {
            $lines[] = $blank;
        }

        return $lines;
    }

    public function handles(): array
    {
        $handles = [];
        $x = $this->contentColumn(0);

        foreach ($this->widths as $i => $width) {
            $x += $width + 2;
            $handles[$this->columnOffset + $i] = $x;
            $x += 1;
        }

        return $handles;
    }

    public function rowIndexFor(int $localRow): ?int
    {
        $target = $this->rowStart + $localRow - 2;

        return $target >= 0 && $target < count($this->rows) ? $target : null;
    }

    public function columnIndexFor(int $localColumn): ?int
    {
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

    public function naturalWidth(string $column): int
    {
        $width = mb_strlen($column);

        foreach ($this->rows as $row) {
            $width = max($width, mb_strlen((string) ($row[$column] ?? '')));
        }

        return $width;
    }

    private function headerLine(): string
    {
        $cells = [];

        foreach (array_slice($this->headers, $this->columnOffset, count($this->widths)) as $i => $name) {
            $text = ' '.$this->style->pad($this->style->truncate($name, $this->widths[$i]), $this->widths[$i]).' ';

            $cells[] = ($this->columnOffset + $i) === $this->columnIndex
                ? $this->style->bold($text)
                : $this->style->dim($text);
        }

        return implode($this->style->dim('│'), $cells);
    }

    private function rule(int $innerWidth): string
    {
        $segments = array_map(fn (int $w) => str_repeat('─', $w + 2), $this->widths);

        $body = implode('┼', $segments);

        return $this->style->dim($body.str_repeat('─', max(0, $innerWidth - mb_strlen($body))));
    }

    private function blankLine(): string
    {
        $cells = array_map(fn (int $w) => str_repeat(' ', $w + 2), $this->widths);

        return implode($this->style->dim('│'), $cells);
    }

    private function rowLine(array $row, int $absolute, int $innerWidth): string
    {
        $selected = $absolute === $this->rowIndex;
        $values = array_values($row);
        $cells = [];

        foreach ($this->widths as $i => $width) {
            $column = $this->columnOffset + $i;
            $value = (string) ($values[$column] ?? '');

            $text = $this->editing !== null && $selected && $column === $this->columnIndex
                ? $this->editBuffer($this->editing, $width)
                : $this->style->truncate($value, $width);

            $padded = ' '.$this->style->pad($text, $width).' ';

            $cells[] = $selected && $column === $this->columnIndex
                ? $this->style->inverse($padded)
                : $padded;
        }

        $line = implode($this->style->dim('│'), $cells);

        return $selected && $this->editing === null
            ? $this->style->underline($this->style->pad($line, $innerWidth))
            : $line;
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

            if ($used + $cost > $available && $widths !== []) {
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

        foreach ($widths as $i => $width) {
            $name = $this->headers[$this->columnOffset + $i] ?? null;

            if ($name === null || isset($this->overrides[$name])) {
                continue;
            }

            $wanted = $this->naturalWidth($name) - $width;

            if ($wanted <= 0) {
                continue;
            }

            $give = min($wanted, $leftover);

            $widths[$i] += $give;
            $leftover -= $give;

            if ($leftover <= 0) {
                break;
            }
        }

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
