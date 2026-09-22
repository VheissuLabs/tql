<?php

namespace App\Prompts\Renderers;

use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\ConnectionForm;
use App\Tui\ConnectionPicker;
use App\Tui\Islands\PickerIsland;
use App\Tui\Islands\Screen;
use App\Tui\Islands\Styler;
use App\Tui\Layout;
use App\Tui\Picker;
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
            $this->titleRule($widths, $inner),
            $this->headerRow($widths, $inner),
            $this->rule($prompt, '├', '┼', '┤', $widths, $inner),
        ];

        $rows = $this->rows($prompt, $widths, $bodyHeight);
        $blank = $this->blank($widths, $inner);

        for ($i = 0; $i < $bodyHeight; $i++) {
            $lines[] = $this->paint(Theme::border(true), '│')
                .$this->pad($rows[$i] ?? $blank, $inner)
                .$this->paint(Theme::border(true), '│');
        }

        $lines[] = $this->rule($prompt, '└', '┴', '┘', $widths, $inner);

        if ($prompt->form !== null) {
            $lines = $this->overlayForm($lines, $prompt->form, $width);

            if ($prompt->form->picker !== null) {
                $lines = $this->overlayPicker($lines, $prompt->form->picker, $width);
            }
        }

        foreach ($lines as $line) {
            $this->line($line);
        }

        $this->line($this->fit($this->dim($prompt->form !== null
            ? ' ↑↓ Move    ↵ Select'
            : ' ↑↓ Move    ↵ Open    e Edit    n New    d Mark    :w Write    :q Quit'), $width));
        $this->line($this->fit($this->status($prompt), $width));

        return $this;
    }

    /**
     * The picker draws itself, so it needs the same styling hooks the browser
     * gives its islands.
     */
    private function styler(): Styler
    {
        return new Styler(
            fn (string $t) => $this->dim($t),
            fn (string $t) => $this->bold($t),
            fn (string $t) => $this->inverse($t),
            fn (string $t) => $this->underline($t),
            fn (string $t, int $w) => $this->truncate($t, $w),
            fn (string $name, string $t) => match ($name) {
                'cursor' => $this->highlight($t, Theme::cursor()),
                'selection' => $this->highlight($t, Theme::selection()),
                default => $t,
            },
        );
    }

    /**
     * @param  array<int, string>  $lines
     * @return array<int, string>
     */
    private function overlayPicker(array $lines, Picker $picker, int $width): array
    {
        $box = new PickerIsland($picker, $this->styler());
        $box->modal = true;

        $boxWidth = min($width - 6, 52);
        $boxHeight = $box->rows();

        // Centre it, then pull it back inside the frame so the last row is
        // not clipped off the bottom.
        $y = (int) ((count($lines) - $boxHeight) / 2);
        $y = max(0, min($y, count($lines) - $boxHeight));

        $box->place(
            max(1, (int) (($width - $boxWidth) / 2) + 1),
            $y,
            $boxWidth,
            $boxHeight,
        );

        $rows = $this->pickerBox($box, $boxWidth);

        foreach ($rows as $i => $row) {
            $at = $box->y + $i;

            if (isset($lines[$at])) {
                $lines[$at] = Screen::splice($lines[$at], $row, $box->x, $boxWidth);
            }
        }

        return $lines;
    }

    /**
     * @return array<int, string>
     */
    private function pickerBox(PickerIsland $box, int $width): array
    {
        $inner = $width - 2;
        $edge = fn (string $text) => $this->paint(Theme::border(true, true), $text);
        $title = ' '.$box->title.' ';

        $rows = [$edge('┌─').$this->bold($this->paint(Theme::title(true, true), $title))
            .$edge(str_repeat('─', max(0, $inner - mb_strlen($title) - 1)).'┐')];

        foreach ($box->content($inner, $box->height - 2) as $line) {
            $rows[] = $edge('│').$this->pad($line, $inner).$edge('│');
        }

        $rows[] = $edge('└'.str_repeat('─', $inner).'┘');

        return $rows;
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
        $title = $form->creating ? ' NEW CONNECTION ' : ' EDIT CONNECTION ';

        $rows = [$edge('┌─').$this->bold($this->paint(Theme::title(true), $title))
            .$edge(str_repeat('─', max(0, $inner - mb_strlen($title) - 1)).'┐')];

        $rows[] = $this->row('', $inner);

        foreach ($labels as $key => $text) {
            $focused = $key === $form->currentKey();
            $shown = $this->truncate($form->display($key), $value);

            if ($focused && $form->editing) {
                $shown = $this->withCursor($shown, $form->cursor());
            }

            if ($key === 'driver' && $focused) {
                $shown = '← '.$shown.' →';
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
            : match (true) {
                // Say what the row under the cursor does. Save and Cancel are
                // rows of their own, so repeating them here is noise.
                $form->editing => '↵ keeps it    esc drops it',
                $form->currentKey() === 'driver' => '← → driver    ctrl+s save    esc cancel',
                default => '↵ change    ctrl+s save    esc cancel',
            }), $inner);

        $rows[] = $edge('└'.str_repeat('─', $inner).'┘');

        return $rows;
    }

    /**
     * Render the character under the cursor in inverse rather than inserting
     * a block, which would shift everything after it along by one.
     */
    private function withCursor(string $text, int $at): string
    {
        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        if ($at >= count($chars)) {
            return $text."\e[7m \e[27m";
        }

        $chars[$at] = "\e[7m".$chars[$at]."\e[27m";

        return implode('', $chars);
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

        // The name column carries the row marker and the driver icon.
        $name = 8;
        $used = 9;
        $tag = 3;

        foreach ($rows as $row) {
            $name = max($name, mb_strlen($row['name']) + 4);
            $used = max($used, mb_strlen($row['used']));
            $tag = max($tag, mb_strlen($this->tagOf($row)));
        }

        $name = min($name, 32);
        $used = min($used, 20);
        $tag = min($tag, 16);

        $where = max(10, $inner - $name - $tag - $used - 11);

        return [$name, $tag, $where, $used];
    }

    private function rule(ConnectionPicker $prompt, string $left, string $join, string $right, array $widths, int $inner): string
    {
        $body = $this->ruleBody($widths, $join, $inner);

        return $this->paint(Theme::border(true), $left)
            .$this->paint(Theme::grid(true), $body)
            .$this->paint(Theme::border(true), $right);
    }

    private function ruleBody(array $widths, string $join, int $inner): string
    {
        $segments = array_map(fn (int $w) => str_repeat('─', $w + 2), $widths);

        $body = implode($join, $segments);

        return $body.str_repeat('─', max(0, $inner - mb_strlen($body)));
    }

    /**
     * The same titled frame the browser panes use, so the first screen looks
     * like the rest of the application rather than a plain table.
     */
    private function titleRule(array $widths, int $inner): string
    {
        $title = ' CONNECTIONS ';
        $body = $this->ruleBody($widths, '┬', $inner);

        $edge = fn (string $text) => $this->paint(Theme::border(true), $text);

        return $edge('┌─')
            .$this->bold($this->paint(Theme::title(true), $title))
            .$this->paint(Theme::grid(true), mb_substr($body, mb_strlen($title) + 1))
            .$edge('┐');
    }

    private function headerRow(array $widths, int $inner): string
    {
        $labels = ['NAME', 'TAG', 'WHERE', 'LAST USED'];
        $cells = [];

        foreach ($widths as $i => $width) {
            $cells[] = $this->bold(' '.$this->pad($this->truncate($labels[$i], $width), $width).' ');
        }

        $grid = $this->paint(Theme::grid(true), '│');

        return $this->paint(Theme::border(true), '│')
            .$this->pad(implode($grid, $cells), $inner)
            .$this->paint(Theme::border(true), '│');
    }

    private function blank(array $widths, int $inner): string
    {
        $cells = array_map(fn (int $w) => str_repeat(' ', $w + 2), $widths);

        return $this->pad(implode($this->paint(Theme::grid(true), '│'), $cells), $inner);
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

        $grid = $this->paint(Theme::grid(true), '│');
        $inner = array_sum($widths) + 3 * count($widths) - 1;

        foreach (array_slice($rows, $start, $height) as $offset => $row) {
            $selected = ($start + $offset) === $prompt->index;
            $marked = $prompt->isMarked((int) $row['id']);
            $values = [$row['name'], $this->tagOf($row), $row['where'], $row['used']];
            $cells = [];

            foreach ($widths as $i => $width) {
                if ($i === 0) {
                    $cells[] = $this->nameCell($row['driver'], (string) $values[0], $width, $selected, $marked);

                    continue;
                }

                $text = ' '.$this->pad($this->truncate((string) $values[$i], $width), $width).' ';

                $cells[] = $selected || $marked ? $text : $this->dim($text);
            }

            // A selected row is built without any colour of its own: an escape
            // sequence inside the span would reset the highlight partway and
            // tear it at the column separators.
            $lines[] = match (true) {
                $marked => $this->highlight(
                    $this->pad(implode('│', $cells), $inner),
                    Theme::colour('deleted', 'red'),
                ),
                $selected => $this->highlight($this->pad(implode('│', $cells), $inner)),
                default => implode($grid, $cells),
            };
        }

        return $lines;
    }

    /**
     * The driver rides in front of the name as a glyph rather than taking a
     * column of its own. Shape carries the meaning as well as colour, so it
     * still reads without colour.
     */
    private function nameCell(string $driver, string $name, int $width, bool $selected, bool $marked = false): string
    {
        $icon = $this->driverIcon($driver);

        $marker = Layout::rowStyle() === 'marker' && $selected ? '▸' : ' ';
        $label = $this->pad($this->truncate($name, $width - 4), $width - 4);

        // A highlighted row carries no colour of its own, marked or selected:
        // the icon's escape code would end the highlight right after it.
        $plain = $selected || $marked;

        return ' '.$marker.' '
            .($plain ? $icon : $this->paint($this->driverColour($driver), $icon))
            .' '.$label.' ';
    }

    /**
     * @param  array<string, mixed>  $row
     */
    private function tagOf(array $row): string
    {
        $tag = trim((string) ($row['tag'] ?? ''));

        return ($row['read_only'] ?? false)
            ? trim($tag.' ro')
            : $tag;
    }

    private function driverIcon(string $driver): string
    {
        return Theme::icon($driver);
    }

    private function driverColour(string $driver): string
    {
        return match ($driver) {
            'mysql' => 'yellow',
            'pgsql' => 'blue',
            'sqlite' => 'green',
            'sqlsrv' => 'magenta',
            default => 'dim',
        };
    }

    private function highlight(string $line, ?string $colour = null): string
    {
        $colour ??= Theme::selection();

        return $colour === 'default'
            ? $this->inverse($line)
            : $this->paint($colour, $this->inverse($line));
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

    private function strip(string $text): string
    {
        return (string) preg_replace('/\e\[[0-9;]*m/', '', $text);
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }
}
