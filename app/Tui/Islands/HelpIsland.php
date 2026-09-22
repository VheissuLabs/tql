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
                'tab' => 'switch between the tables list and the rows',
                '↑ ↓ / j k' => 'move the cursor',
                '← → / h l' => 'move between columns',
                'n / p' => 'next or previous page',
                'r' => 'reload the current table',
            ],
            'doing' => [
                '↵' => 'open a table, or edit the selected value',
                'i' => 'view the value full screen, never writes',
                'e' => 'edit the value, ctrl+s saves',
                's' => 'open the SQL editor (ctrl+r runs it)',
                '< >' => 'narrow or widen the selected column',
                '=' => 'reset the column width',
            ],
            'mouse' => [
                'click' => 'select a table, row or cell',
                'drag' => 'a column border in the header resizes it',
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
