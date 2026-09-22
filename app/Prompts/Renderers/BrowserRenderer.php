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

    private const CHROME = Layout::CHROME;

    public function __invoke(Browser $prompt): string
    {
        $prompt->firstBodyRow ??= Layout::firstBodyRow(max(2 - $prompt->newLinesWritten(), 0));

        $width = max(60, $prompt->terminal()->cols());
        $height = max(10, $prompt->terminal()->lines());

        $gridWidth = $width - self::SIDEBAR - self::CHROME;
        $bodyHeight = max(3, $height - 7);

        $sidebar = $this->sidebar($prompt, $bodyHeight);
        $grid = $this->grid($prompt, $gridWidth, $bodyHeight);

        $this->line($this->topBorder($prompt, $gridWidth));

        for ($i = 0; $i < $bodyHeight; $i++) {
            $this->line(
                $this->dim('│').' '.
                $this->pad($sidebar[$i] ?? '', self::SIDEBAR).' '.
                $this->dim('│').' '.
                $this->pad($grid[$i] ?? '', $gridWidth).' '.
                $this->dim('│')
            );
        }

        $this->line($this->bottomBorder($gridWidth));
        $this->line($this->status($prompt));

        $this->clearHotkeys();
        $this->hotkey('tab', 'Pane');
        $this->hotkey('↑↓', 'Move');
        $this->hotkey('←→', 'Columns');
        $this->hotkey('↵', 'Open');
        $this->hotkey('e', 'Edit');
        $this->hotkey('< >', 'Width');
        $this->hotkey('r', 'Reload');
        $this->hotkey('n/p', 'Page');
        $this->hotkey(':q', 'Quit');

        collect($this->hotkeys())->each($this->line(...));

        return $this;
    }

    private function topBorder(Browser $prompt, int $gridWidth): string
    {
        $left = $this->cap('TABLES', self::SIDEBAR, $prompt->focus === 'sidebar');
        $right = $this->cap(
            $prompt->currentTable() ?? $prompt->connection->name,
            $gridWidth,
            $prompt->focus === 'grid'
        );

        return $this->dim('┌').$left.$this->dim('┬').$right.$this->dim('┐');
    }

    private function cap(string $title, int $inner, bool $focused): string
    {
        $label = ' '.$this->truncate($title, max(1, $inner - 4)).' ';
        $label = $focused ? $this->bold($label) : $this->dim($label);

        $used = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $label));

        return $this->dim('─').$label.$this->dim(str_repeat('─', max(0, $inner + 2 - $used - 1)));
    }

    private function bottomBorder(int $gridWidth): string
    {
        return $this->dim(
            '└'.str_repeat('─', self::SIDEBAR + 2).'┴'.str_repeat('─', $gridWidth + 2).'┘'
        );
    }

    private function status(Browser $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.'█';
        }

        if ($prompt->editing !== null) {
            $column = $prompt->headers[$prompt->columnIndex] ?? '?';

            return ' '.$this->bold("editing {$column}").$this->dim('  ↵ save    esc cancel');
        }

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnOffset + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position);
    }

    private function sidebar(Browser $prompt, int $height): array
    {
        $lines = [];
        $start = $this->windowStart($prompt->tableIndex, count($prompt->tables), $height);

        $prompt->sidebarStart = $start;

        foreach (array_slice($prompt->tables, $start, $height) as $index => $table) {
            $actual = $start + $index;
            $label = $this->truncate($table, self::SIDEBAR - 2);

            $lines[] = $actual === $prompt->tableIndex
                ? $this->inverse($this->pad(' '.$label, self::SIDEBAR))
                : ' '.$this->dim($label);
        }

        return $lines;
    }

    private function grid(Browser $prompt, int $width, int $height): array
    {
        if ($prompt->headers === []) {
            return [$this->dim('no columns')];
        }

        $widths = $this->columnWidths($prompt, $width);
        $visible = count($widths);

        $prompt->visibleColumns = $visible;
        $prompt->columnHandles = $this->handles($prompt, $widths);

        $header = [];

        foreach ($prompt->visibleHeaders($visible) as $i => $name) {
            $label = $this->pad($this->truncate($name, $widths[$i]), $widths[$i]);

            $header[] = ($prompt->columnOffset + $i) === $prompt->columnIndex && $prompt->focus === 'grid'
                ? $this->underline($label)
                : $label;
        }

        $headerLine = implode('  ', $header);

        $lines = [
            $this->bold($headerLine),
            $this->dim(str_repeat('─', $width)),
        ];

        $room = max(1, $height - count($lines));
        $start = $this->windowStart($prompt->rowIndex, count($prompt->rows), $room);

        $prompt->gridStart = $start;

        foreach (array_slice($prompt->rows, $start, $room) as $index => $row) {
            $cells = [];

            $selected = ($start + $index) === $prompt->rowIndex && $prompt->focus === 'grid';

            foreach ($prompt->visibleRow($row, $visible) as $i => $cell) {
                $absolute = $prompt->columnOffset + $i;

                if ($prompt->editingCell($start + $index, $absolute)) {
                    $cells[] = $this->pad($this->editBuffer($prompt->editing, $widths[$i]), $widths[$i]);

                    continue;
                }

                $text = $this->pad($this->truncate((string) $cell, $widths[$i]), $widths[$i]);

                $cells[] = $selected && $absolute === $prompt->columnIndex
                    ? $this->underline($text)
                    : $text;
            }

            $text = implode('  ', $cells);

            $lines[] = $selected ? $this->inverse($this->pad($text, $width)) : $text;
        }

        return $lines;
    }

    private function editBuffer(string $buffer, int $width): string
    {
        $text = $buffer.'█';

        return mb_strlen($text) > $width ? mb_substr($text, -$width) : $text;
    }

    private function windowStart(int $cursor, int $total, int $room): int
    {
        if ($total <= $room) {
            return 0;
        }

        return max(0, min($cursor - intdiv($room, 2), $total - $room));
    }

    private function handles(Browser $prompt, array $widths): array
    {
        $handles = [];
        $x = Layout::gridFirstColumn();

        foreach ($widths as $i => $width) {
            $handles[$prompt->columnOffset + $i] = $x + $width;
            $x += $width + 2;
        }

        return $handles;
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
                $width = max($width, min(24, $available - 2));
            }

            $widths[] = min($width, max(3, $available - 2));
        }

        return $widths;
    }

    private function scrollToCursor(Browser $prompt, array $all, int $available): int
    {
        $offset = min($prompt->columnOffset, $prompt->columnIndex);

        while (true) {
            $used = 0;

            for ($i = $offset; $i <= $prompt->columnIndex; $i++) {
                $used += ($all[$i] ?? 0) + 2;
            }

            if ($used <= $available || $offset >= $prompt->columnIndex) {
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
            return [$available];
        }

        $prompt->columnOffset = $this->scrollToCursor($prompt, $all, $available);

        $widths = [];
        $used = 0;

        for ($i = $prompt->columnOffset; $i < count($all); $i++) {
            $width = $all[$i];

            if ($used + $width + 2 > $available && $widths !== []) {
                break;
            }

            $widths[] = $width;
            $used += $width + 2;
        }

        return $widths === [] ? [$available] : $widths;
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        if ($length > $width) {
            return $text;
        }

        return $text.str_repeat(' ', $width - $length);
    }
}
