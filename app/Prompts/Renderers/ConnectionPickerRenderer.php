<?php

namespace App\Prompts\Renderers;

use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\ConnectionPicker;
use App\Tui\Layout;
use Laravel\Prompts\Themes\Default\Renderer;

class ConnectionPickerRenderer extends Renderer
{
    use RendersWithoutPadding;

    public function __invoke(ConnectionPicker $prompt): string
    {
        $prompt->firstBodyRow = Layout::firstBodyRow(Layout::topMargin());

        $width = max(60, $prompt->terminal()->cols());
        $height = max(10, $prompt->terminal()->lines());

        $inner = $width - 2;
        $bodyHeight = max(3, $height - 7 - Layout::topMargin());

        $widths = $this->widths($prompt, $inner);

        foreach (range(0, Layout::topMargin()) as $i) {
            if ($i < Layout::topMargin()) {
                $this->line('');
            }
        }

        $this->line($this->rule($prompt, '┌', '┬', '┐', $widths, $inner));
        $this->line($this->headerRow($widths, $inner));
        $this->line($this->rule($prompt, '├', '┼', '┤', $widths, $inner));

        $rows = $this->rows($prompt, $widths, $bodyHeight);
        $blank = $this->blank($widths, $inner);

        for ($i = 0; $i < $bodyHeight; $i++) {
            $this->line($this->dim('│').$this->pad($rows[$i] ?? $blank, $inner).$this->dim('│'));
        }

        $this->line($this->rule($prompt, '└', '┴', '┘', $widths, $inner));
        $this->line($this->status($prompt));
        $this->line($this->dim(' ↑↓ Move    ↵ Open    e Edit    n New    :q Quit'));

        return $this;
    }

    private function widths(ConnectionPicker $prompt, int $inner): array
    {
        $rows = $prompt->rows();

        $name = 4;
        $driver = 6;
        $used = 9;

        foreach ($rows as $row) {
            $name = max($name, mb_strlen($row['name']));
            $driver = max($driver, mb_strlen($row['driver']));
            $used = max($used, mb_strlen($row['used']));
        }

        $name = min($name, 30);
        $used = min($used, 20);

        $where = max(10, $inner - $name - $driver - $used - 11);

        return [$name, $driver, $where, $used];
    }

    private function rule(ConnectionPicker $prompt, string $left, string $join, string $right, array $widths, int $inner): string
    {
        $segments = array_map(fn (int $w) => str_repeat('─', $w + 2), $widths);

        $body = implode($join, $segments);
        $body .= str_repeat('─', max(0, $inner - mb_strlen($body)));

        return $this->dim($left.$body.$right);
    }

    private function headerRow(array $widths, int $inner): string
    {
        $labels = ['NAME', 'DRIVER', 'WHERE', 'LAST USED'];
        $cells = [];

        foreach ($widths as $i => $width) {
            $cells[] = $this->dim(' '.$this->pad($this->truncate($labels[$i], $width), $width).' ');
        }

        return $this->dim('│').$this->pad(implode($this->dim('│'), $cells), $inner).$this->dim('│');
    }

    private function blank(array $widths, int $inner): string
    {
        $cells = array_map(fn (int $w) => str_repeat(' ', $w + 2), $widths);

        return $this->pad(implode($this->dim('│'), $cells), $inner);
    }

    private function rows(ConnectionPicker $prompt, array $widths, int $height): array
    {
        $rows = $prompt->rows();

        if ($rows === []) {
            return [$this->dim('  No connections yet — press n to add one.')];
        }

        $start = count($rows) <= $height
            ? 0
            : max(0, min($prompt->index - intdiv($height, 2), count($rows) - $height));

        $prompt->start = $start;

        $lines = [];

        foreach (array_slice($rows, $start, $height) as $offset => $row) {
            $values = [$row['name'], $row['driver'], $row['where'], $row['used']];
            $cells = [];

            foreach ($widths as $i => $width) {
                $cells[] = ' '.$this->pad($this->truncate((string) $values[$i], $width), $width).' ';
            }

            $line = implode($this->dim('│'), $cells);

            $lines[] = ($start + $offset) === $prompt->index ? $this->inverse($line) : $line;
        }

        return $lines;
    }

    private function status(ConnectionPicker $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.'█';
        }

        return $this->dim(' '.($prompt->status ?? $prompt->connections->count().' connections'));
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }
}
