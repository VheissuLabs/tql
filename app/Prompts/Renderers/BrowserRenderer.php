<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use Chewie\Concerns\Aligns;
use Chewie\Concerns\DrawsHotkeys;
use Laravel\Prompts\Themes\Default\Renderer;

class BrowserRenderer extends Renderer
{
    use Aligns;
    use DrawsHotkeys;

    private const SIDEBAR = 26;

    private const GUTTER = 3;

    public function __invoke(Browser $prompt): string
    {
        $width = $prompt->terminal()->cols();
        $height = $prompt->terminal()->lines();

        $bodyHeight = max(3, $height - 6);
        $gridWidth = max(20, $width - self::SIDEBAR - self::GUTTER);

        $this->line($this->header($prompt, $width));

        $sidebar = $this->sidebar($prompt, $bodyHeight);
        $grid = $this->grid($prompt, $gridWidth, $bodyHeight);

        for ($i = 0; $i < $bodyHeight; $i++) {
            $left = $this->pad($sidebar[$i] ?? '', self::SIDEBAR);
            $divider = $this->dim(' │ ');

            $this->line($left.$divider.($grid[$i] ?? ''));
        }

        $this->line($this->footer($prompt, $width));

        $this->clearHotkeys();
        $this->hotkey('tab', 'Switch pane');
        $this->hotkey('↑↓/jk', 'Move');
        $this->hotkey('←→/hl', 'Columns');
        $this->hotkey('↵', 'Open');
        $this->hotkey('n/p', 'Page');
        $this->hotkeyQuit();

        collect($this->hotkeys())->each($this->line(...));

        return $this;
    }

    private function header(Browser $prompt, int $width): string
    {
        $name = $this->bold($this->cyan(' '.$prompt->connection->name.' '));
        $where = $this->dim($prompt->connection->describe());

        return $name.$where;
    }

    private function footer(Browser $prompt, int $width): string
    {
        $table = $prompt->currentTable() ?? 'no table';
        $columns = count($prompt->headers);
        $shown = $prompt->columnOffset + 1;

        if ($prompt->command !== null) {
            return $this->cyan(' :'.$prompt->command.'█');
        }

        $left = $this->dim(" {$table}");
        $right = $prompt->status === null
            ? ''
            : $this->dim($prompt->status." · col {$shown}/{$columns}");

        return $left.'  '.$right;
    }

    private function sidebar(Browser $prompt, int $height): array
    {
        $focused = $prompt->focus === 'sidebar';
        $lines = [$focused ? $this->cyan(' TABLES') : $this->dim(' TABLES'), ''];

        $room = $height - count($lines);
        $start = max(0, min($prompt->tableIndex - intdiv($room, 2), count($prompt->tables) - $room));
        $start = max(0, $start);

        foreach (array_slice($prompt->tables, $start, $room) as $index => $table) {
            $actual = $start + $index;
            $label = $this->truncate($table, self::SIDEBAR - 3);

            $lines[] = $actual === $prompt->tableIndex
                ? ($focused ? $this->inverse(' '.$this->pad($label, self::SIDEBAR - 1)) : $this->cyan(' ▸ '.$label))
                : '  '.$this->dim($label);
        }

        return $lines;
    }

    private function grid(Browser $prompt, int $width, int $height): array
    {
        if ($prompt->headers === []) {
            return ['', $this->dim('  no columns')];
        }

        $widths = $this->columnWidths($prompt, $width);
        $visible = count($widths);

        $header = [];

        foreach ($prompt->visibleHeaders($visible) as $i => $name) {
            $header[] = $this->pad($this->truncate($name, $widths[$i]), $widths[$i]);
        }

        $lines = [$this->bold(implode('  ', $header))];
        $lines[] = $this->dim(str_repeat('─', min($width, max(1, mb_strlen(implode('  ', $header))))));

        $room = $height - count($lines);

        $start = max(0, min($prompt->rowIndex - intdiv($room, 2), count($prompt->rows) - $room));
        $start = max(0, $start);

        foreach (array_slice($prompt->rows, $start, $room) as $index => $row) {
            $cells = [];

            foreach ($prompt->visibleRow($row, $visible) as $i => $cell) {
                $cells[] = $this->pad($this->truncate((string) $cell, $widths[$i]), $widths[$i]);
            }

            $text = implode('  ', $cells);

            $lines[] = ($start + $index) === $prompt->rowIndex && $prompt->focus === 'grid'
                ? $this->inverse($text)
                : $text;
        }

        return $lines;
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

        return $text.str_repeat(' ', max(0, $width - $length));
    }
}
