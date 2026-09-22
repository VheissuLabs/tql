<?php

namespace App\Tui\Islands;

class HelpIsland extends Island
{
    public const WIDTH = 72;

    public string $title = 'HELP';

    public function naturalHeight(): int
    {
        return count($this->lines());
    }

    /** Lines scrolled past, so the help can be longer than the screen. */
    public int $hidden = 0;

    public function __construct(private Styler $style, private int $offset = 0) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->lines();

        $this->hidden = max(0, count($lines) - $innerHeight);

        $offset = min($this->offset, $this->hidden);

        $visible = array_slice($lines, $offset, $innerHeight);

        if ($this->hidden > 0) {
            $visible[$innerHeight - 1] = '   '.$this->style->dim(
                $offset < $this->hidden ? 'j / ↓ for more' : 'g returns to the top'
            );
        }

        return $visible;
    }

    /**
     * @return array<int, string>
     */
    private function lines(): array
    {
        $lines = [];

        foreach ($this->sections() as $heading => $entries) {
            $lines[] = ' '.$this->style->bold($heading);

            foreach ($entries as $keys => $description) {
                $lines[] = '   '.$this->style->pad($keys, 14).$this->style->dim($description);
            }

            $lines[] = '';
        }

        return $lines;
    }

    private function sections(): array
    {
        return [
            'moving' => [
                'tab' => 'cycle panes: tables, rows, and SQL when shown',
                'shift+tab' => 'cycle panes the other way',
                'ctrl+l' => 'redraw the screen when something else has messed it up',
                '↑ ↓ / j k' => 'move the cursor',
                '← → / h l' => 'move between columns',
                'n / p' => 'next or previous page',
                'r' => 'reload the current table',
                'o' => 'sort by the column the cursor is on: asc, desc, then back to the primary key',
            ],
            'doing' => [
                '↵' => 'open a table, or edit the selected value',
                'i' => 'view the value full screen, never writes',
                'e' => 'edit the value, ctrl+s saves',
                's' => 'open the SQL editor (ctrl+r runs it)',
                ', .' => 'narrow or widen the selected column (< > work too)',
                '=' => 'reset the column width',
            ],
            'viewing a value (i)' => [
                'j / k' => 'move a line, 3j moves three',
                'g / G' => 'jump to the top or bottom',
                '12G' => 'jump to line 12',
                'click' => 'put the cursor on a line',
                'drag' => 'select a range of lines',
                'V' => 'start a line selection',
                'y' => 'yank the selection to the clipboard',
                'esc' => 'clear the selection, then close',
            ],
            'mouse' => [
                'click' => 'select a table, row or cell',
                'drag' => 'a column border in the header resizes it',
                'header' => 'click a column header to sort by it',
                'wheel' => 'scroll the focused pane',
            ],
            'commands' => [
                ':export' => 'write this table to a .sql file',
                ':sql' => 'open the SQL editor',
                ':tables' => 'focus the tables list',
                ':rows' => 'focus the rows',
                ':reload' => 'reload the current table',
                ':c' => 'back to the connection list',
                ':q' => 'quit dotsql',
            ],
            'from the shell' => [
                'export' => 'dotsql export <connection> [table] --limit= --sql=',
                '' => 'dotsql export <connection> --list',
            ],
        ];
    }
}
