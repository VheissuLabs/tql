<?php

namespace App\Tui\Islands;

class HelpIsland extends Island
{
    public string $title = 'HELP';

    public function __construct(private Styler $style) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [];

        foreach ($this->sections() as $heading => $entries) {
            $lines[] = ' '.$this->style->bold($heading);

            foreach ($entries as $keys => $description) {
                $lines[] = '   '.$this->style->pad($keys, 14).$this->style->dim($description);
            }

            $lines[] = '';
        }

        return array_slice($lines, 0, $innerHeight);
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
                'o' => 'sort by the column the cursor is on',
            ],
            'doing' => [
                '↵' => 'open a table, or edit the selected value',
                'i' => 'view the value full screen, never writes',
                'e' => 'edit the value, ctrl+s saves',
                's' => 'open the SQL editor (ctrl+r runs it)',
                '< >' => 'narrow or widen the selected column',
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
                ':mouse' => 'show raw click coordinates for debugging',
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
