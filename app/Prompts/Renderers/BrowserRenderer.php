<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use App\Tui\Layout;
use Chewie\Concerns\DrawsHotkeys;
use Laravel\Prompts\Themes\Default\Renderer;

class BrowserRenderer extends Renderer
{
    use DrawsHotkeys;

    private const SIDEBAR = Layout::SIDEBAR;

    public function __invoke(Browser $prompt): string
    {
        $prompt->firstBodyRow ??= Layout::firstBodyRow(max(2 - $prompt->newLinesWritten(), 0));

        $width = max(60, $prompt->terminal()->cols());
        $height = max(10, $prompt->terminal()->lines());

        $available = $width - self::SIDEBAR - 3;
        $bodyHeight = max(3, $height - 9);

        $widths = $this->columnWidths($prompt, $available);

        $prompt->visibleColumns = count($widths);
        $prompt->columnHandles = $this->handles($prompt, $widths);

        $this->line($this->rule($prompt, '┌', '┬', '┐', $widths, $available));
        $this->line($this->headerRow($prompt, $widths, $available));
        $this->line($this->rule($prompt, '├', '┼', '┤', $widths, $available));

        $sidebar = $this->sidebar($prompt, $bodyHeight);
        $rows = $this->dataRows($prompt, $widths, $bodyHeight, $available);

        $blank = $this->blankRow($widths, $available);

        for ($i = 0; $i < $bodyHeight; $i++) {
            $this->line(
                $this->dim('│').
                $this->pad($sidebar[$i] ?? '', self::SIDEBAR).
                $this->dim('│').
                $this->pad($rows[$i] ?? $blank, $available).
                $this->dim('│')
            );
        }

        $this->line($this->rule($prompt, '└', '┴', '┘', $widths, $available));
        $this->line($this->status($prompt));

        $this->clearHotkeys();
        $this->hotkey('tab', 'Pane');
        $this->hotkey('↑↓←→', 'Move');
        $this->hotkey('e', 'Edit');
        $this->hotkey('< >', 'Width');
        $this->hotkey('r', 'Reload');
        $this->hotkey('n/p', 'Page');
        $this->hotkey(':q', 'Quit');

        collect($this->hotkeys())->each($this->line(...));

        return $this;
    }

    private function rule(Browser $prompt, string $left, string $join, string $right, array $widths, int $available): string
    {
        $segments = [];

        foreach ($widths as $width) {
            $segments[] = str_repeat('─', $width + 2);
        }

        $body = $segments === [] ? '' : implode($join, $segments);
        $body .= str_repeat('─', max(0, $available - mb_strlen($body)));

        return $this->dim($left.str_repeat('─', self::SIDEBAR).$join.$body.$right);
    }

    private function headerRow(Browser $prompt, array $widths, int $available): string
    {
        $cells = [];

        foreach ($prompt->visibleHeaders(count($widths)) as $i => $name) {
            $absolute = $prompt->columnOffset + $i;
            $text = ' '.$this->pad($this->truncate($name, $widths[$i]), $widths[$i]).' ';

            $cells[] = $absolute === $prompt->columnIndex
                ? $this->bold($text)
                : $this->dim($text);
        }

        $body = $cells === []
            ? $this->pad('', $available)
            : $this->pad(implode($this->dim('│'), $cells), $available);

        return $this->dim('│').
            $this->pad(' '.$this->bold('TABLES'), self::SIDEBAR).
            $this->dim('│').
            $body.
            $this->dim('│');
    }

    private function dataRows(Browser $prompt, array $widths, int $height, int $available): array
    {
        if ($prompt->rows === []) {
            return [$this->dim(' no rows')];
        }

        $start = $this->windowStart($prompt->rowIndex, count($prompt->rows), $height);

        $prompt->gridStart = $start;

        $lines = [];

        foreach (array_slice($prompt->rows, $start, $height) as $index => $row) {
            $selected = ($start + $index) === $prompt->rowIndex && $prompt->focus === 'grid';
            $cells = [];

            foreach ($prompt->visibleRow($row, count($widths)) as $i => $cell) {
                $absolute = $prompt->columnOffset + $i;

                $text = $prompt->editingCell($start + $index, $absolute)
                    ? $this->editBuffer($prompt->editing, $widths[$i])
                    : $this->truncate((string) $cell, $widths[$i]);

                $padded = ' '.$this->pad($text, $widths[$i]).' ';

                $cells[] = $selected && $absolute === $prompt->columnIndex
                    ? $this->inverse($padded)
                    : $padded;
            }

            $line = implode($this->dim('│'), $cells);

            $lines[] = $selected && $prompt->editing === null
                ? $this->underline($this->pad($line, $available))
                : $line;
        }

        return $lines;
    }

    private function blankRow(array $widths, int $available): string
    {
        if ($widths === []) {
            return '';
        }

        $cells = array_map(fn (int $width) => str_repeat(' ', $width + 2), $widths);

        return $this->pad(implode($this->dim('│'), $cells), $available);
    }

    private function handles(Browser $prompt, array $widths): array
    {
        $handles = [];
        $x = Layout::gridFirstColumn();

        foreach ($widths as $i => $width) {
            $x += $width + 2;
            $handles[$prompt->columnOffset + $i] = $x;
            $x += 1;
        }

        return $handles;
    }

    private function status(Browser $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.'█';
        }

        if ($prompt->editing !== null) {
            $column = $prompt->headers[$prompt->columnIndex] ?? '?';

            return ' '.$this->bold("editing {$column}").$this->dim('   ↵ save    esc cancel');
        }

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnIndex + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position);
    }

    private function sidebar(Browser $prompt, int $height): array
    {
        $lines = [];
        $start = $this->windowStart($prompt->tableIndex, count($prompt->tables), $height);

        $prompt->sidebarStart = $start;

        foreach (array_slice($prompt->tables, $start, $height) as $index => $table) {
            $label = $this->truncate($table, self::SIDEBAR - 2);

            $lines[] = ($start + $index) === $prompt->tableIndex
                ? $this->inverse($this->pad(' '.$label, self::SIDEBAR))
                : ' '.$this->dim($label);
        }

        return $lines;
    }

    private function windowStart(int $cursor, int $total, int $room): int
    {
        if ($total <= $room) {
            return 0;
        }

        return max(0, min($cursor - intdiv($room, 2), $total - $room));
    }

    private function allWidths(Browser $prompt, int $available): array
    {
        $widths = [];

        foreach ($prompt->headers as $index => $name) {
            $width = mb_strlen($name);

            foreach ($prompt->rows as $row) {
                $values = array_values($row);
                $width = max($width, mb_strlen((string) ($values[$index] ?? '')));
            }

            $width = $prompt->widthFor($name, min($width, 28));

            if ($prompt->editing !== null && $index === $prompt->columnIndex) {
                $width = max($width, min(24, $available - 3));
            }

            $widths[] = max(3, min($width, $available - 3));
        }

        return $widths;
    }

    private function scrollToCursor(Browser $prompt, array $all, int $available): int
    {
        $offset = min($prompt->columnOffset, $prompt->columnIndex);

        while ($offset < $prompt->columnIndex) {
            $used = 0;

            for ($i = $offset; $i <= $prompt->columnIndex; $i++) {
                $used += ($all[$i] ?? 0) + 3;
            }

            if ($used <= $available) {
                break;
            }

            $offset++;
        }

        return $offset;
    }

    private function columnWidths(Browser $prompt, int $available): array
    {
        $all = $this->allWidths($prompt, $available);

        if ($all === []) {
            return [];
        }

        $prompt->columnOffset = $this->scrollToCursor($prompt, $all, $available);

        $widths = [];
        $used = 0;

        for ($i = $prompt->columnOffset; $i < count($all); $i++) {
            $cost = $all[$i] + 2 + ($widths === [] ? 0 : 1);

            if ($used + $cost > $available && $widths !== []) {
                break;
            }

            $widths[] = $all[$i];
            $used += $cost;
        }

        return $widths;
    }

    private function editBuffer(string $buffer, int $width): string
    {
        $text = $buffer.'█';

        return mb_strlen($text) > $width ? mb_substr($text, -$width) : $text;
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }
}
