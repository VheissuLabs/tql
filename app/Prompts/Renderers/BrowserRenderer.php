<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use Chewie\Concerns\DrawsHotkeys;
use Laravel\Prompts\Themes\Default\Renderer;

class BrowserRenderer extends Renderer
{
    use DrawsHotkeys;

    private const SIDEBAR = 24;

    private const CHROME = 7;

    public function __invoke(Browser $prompt): string
    {
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

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnOffset + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position);
    }

    private function sidebar(Browser $prompt, int $height): array
    {
        $lines = [];
        $start = $this->windowStart($prompt->tableIndex, count($prompt->tables), $height);

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

        $header = [];

        foreach ($prompt->visibleHeaders($visible) as $i => $name) {
            $header[] = $this->pad($this->truncate($name, $widths[$i]), $widths[$i]);
        }

        $headerLine = implode('  ', $header);

        $lines = [
            $this->bold($headerLine),
            $this->dim(str_repeat('─', $width)),
        ];

        $room = max(1, $height - count($lines));
        $start = $this->windowStart($prompt->rowIndex, count($prompt->rows), $room);

        foreach (array_slice($prompt->rows, $start, $room) as $index => $row) {
            $cells = [];

            foreach ($prompt->visibleRow($row, $visible) as $i => $cell) {
                $cells[] = $this->pad($this->truncate((string) $cell, $widths[$i]), $widths[$i]);
            }

            $text = implode('  ', $cells);

            $lines[] = ($start + $index) === $prompt->rowIndex && $prompt->focus === 'grid'
                ? $this->inverse($this->pad($text, $width))
                : $text;
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

    private function columnWidths(Browser $prompt, int $available): array
    {
        $widths = [];
        $used = 0;

        foreach (array_slice($prompt->headers, $prompt->columnOffset) as $index => $name) {
            $width = mb_strlen($name);

            foreach ($prompt->rows as $row) {
                $values = array_values($row);
                $width = max($width, mb_strlen((string) ($values[$prompt->columnOffset + $index] ?? '')));
            }

            $width = min($width, 28);

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
