<?php

namespace App\Prompts\Renderers;

use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\ConnectionForm;
use App\Tui\ConnectionPicker;
use App\Tui\Islands\Screen;
use App\Tui\Layout;
use App\Tui\Theme;
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

        $lines = [
            $this->rule($prompt, '┌', '┬', '┐', $widths, $inner),
            $this->headerRow($widths, $inner),
            $this->rule($prompt, '├', '┼', '┤', $widths, $inner),
        ];

        $rows = $this->rows($prompt, $widths, $bodyHeight);
        $blank = $this->blank($widths, $inner);

        for ($i = 0; $i < $bodyHeight; $i++) {
            $lines[] = $this->dim('│').$this->pad($rows[$i] ?? $blank, $inner).$this->dim('│');
        }

        $lines[] = $this->rule($prompt, '└', '┴', '┘', $widths, $inner);

        if ($prompt->form !== null) {
            $lines = $this->overlayForm($lines, $prompt->form, $width);
        }

        foreach ($lines as $line) {
            $this->line($line);
        }

        $this->line($this->fit($this->dim($prompt->form !== null
            ? ' ↑↓ Field    ↵ Change    ctrl+s Save    esc Cancel'
            : ' ↑↓ Move    ↵ Open    e Edit    n New    :q Quit'), $width));
        $this->line($this->fit($this->status($prompt), $width));

        return $this;
    }

    /**
     * Float the edit form over the list, centred, using the same splice the
     * help modal uses so the rows behind it keep their styling.
     *
     * @param  array<int, string>  $lines
     * @return array<int, string>
     */
    private function overlayForm(array $lines, ConnectionForm $form, int $width): array
    {
        $box = $this->formBox($form, $modalWidth);

        $x = max(1, (int) (($width - $modalWidth) / 2) + 1);
        $y = max(0, (int) ((count($lines) - count($box)) / 2));

        foreach ($box as $i => $row) {
            if (isset($lines[$y + $i])) {
                $lines[$y + $i] = Screen::splice($lines[$y + $i], $row, $x, $modalWidth);
            }
        }

        return $lines;
    }

    /**
     * @return array<int, string>
     */
    private function formBox(ConnectionForm $form, ?int &$modalWidth): array
    {
        $labels = $form->fields();
        $label = max(array_map('mb_strlen', $labels));
        $value = 34;

        $modalWidth = $label + $value + 7;
        $inner = $modalWidth - 2;

        $edge = fn (string $text) => $this->paint(Theme::border(true), $text);
        $title = ' EDIT '.strtoupper($form->connection->driver).' CONNECTION ';

        $rows = [$edge('┌─').$this->bold($this->paint(Theme::title(true), $title))
            .$edge(str_repeat('─', max(0, $inner - mb_strlen($title) - 1)).'┐')];

        $rows[] = $this->row('', $inner);

        foreach ($labels as $key => $text) {
            $focused = $key === $form->currentKey();
            $shown = $this->truncate($form->display($key), $value);

            if ($focused && $form->editing) {
                $shown .= "\e[7m \e[27m";
            }

            $rows[] = $this->row(
                '  '.($focused ? $this->bold($this->pad($text, $label)) : $this->dim($this->pad($text, $label)))
                .'  '.($focused ? $shown : $this->dim($shown)),
                $inner,
            );
        }

        $rows[] = $this->row('', $inner);

        $rows[] = $this->row('  '.$this->dim($form->error !== null
            ? $this->paint('red', $form->error)
            : ($form->editing ? '↵ keeps it    esc drops it' : '↵ change    ctrl+s save    esc cancel')), $inner);

        $rows[] = $edge('└'.str_repeat('─', $inner).'┘');

        return $rows;
    }

    private function row(string $content, int $inner): string
    {
        $edge = fn (string $text) => $this->paint(Theme::border(true), $text);

        return $edge('│').$this->pad($content, $inner).$edge('│');
    }

    private function paint(string $colour, string $text): string
    {
        return match ($colour) {
            'default' => $text,
            'red' => $this->red($text),
            'green' => $this->green($text),
            'yellow' => $this->yellow($text),
            'blue' => $this->blue($text),
            'magenta' => $this->magenta($text),
            'cyan' => $this->cyan($text),
            'white' => $this->white($text),
            'gray' => $this->gray($text),
            default => $this->dim($text),
        };
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

    /**
     * Neither of the last two lines is inside the frame, so nothing else stops
     * them overflowing, and a wrap costs a row the frame does not know about.
     */
    private function fit(string $line, int $width): string
    {
        $visible = mb_strlen((string) preg_replace('/\e\[[0-9;]*m/', '', $line));

        return $visible <= $width ? $line : $this->truncate($line, $width);
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }
}
