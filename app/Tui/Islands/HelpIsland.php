<?php

namespace App\Tui\Islands;

class HelpIsland extends Island
{
    public const WIDTH = 56;

    private const KEYS = 12;

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
        $lines = $this->lines($innerWidth);

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
     * Every line is cut to the modal. A line that overruns wraps, and a wrap
     * costs a terminal row the frame does not know about.
     *
     * @return array<int, string>
     */
    private function lines(int $width = self::WIDTH - 2): array
    {
        $lines = [];

        foreach ($this->sections() as $heading => $entries) {
            $lines[] = ' '.$this->style->bold($this->style->truncate($heading, $width - 1));

            foreach ($entries as $keys => $description) {
                $lines[] = '   '.$this->style->pad($this->style->truncate($keys, self::KEYS), self::KEYS)
                    .$this->style->dim($this->style->truncate($description, $width - self::KEYS - 3));
            }

            $lines[] = '';
        }

        return $lines;
    }

    private function sections(): array
    {
        return [
            'moving' => [
                'tab' => 'next pane',
                'shift+tab' => 'previous pane',
                '↑ ↓ / j k' => 'move the cursor',
                '← → / h l' => 'move between columns',
                'n / p' => 'next or previous page',
                'o' => 'sort this column: asc, desc, primary key',
                'r' => 'reload the table',
                'd' => 'mark the row for deletion',
                'u' => 'clear every mark',
                'ctrl+l' => 'redraw the screen',
            ],
            'doing' => [
                '↵' => 'open a table, or edit the value',
                'i' => 'inspect the row: record and related, foldable',
                'I' => 'view just this value, read only',
                'e' => 'edit the value, ctrl+s saves',
                's' => 'SQL editor, ctrl+r runs it',
                'a' => 'ask for a query in plain english',
                '/' => 'filter the tables list',
                'f' => 'filter the rows: column, operator, value',
                't' => 'structure: columns, types, keys and indexes',
                'L' => 'follow a link: the key under the cursor, or what points here',
                'esc / ctrl+o' => 'go back where you followed from',
                '↵ in a filter' => 'open a list you can type to narrow',
                ', .' => 'narrow or widen the column',
                '=' => 'reset the column width',
            ],
            'viewing a value' => [
                'j / k' => 'move a line, 3j moves three',
                'g / G' => 'top or bottom',
                '12G' => 'jump to line 12',
                'V' => 'start a line selection',
                'y' => 'yank the selection',
                'esc' => 'clear the selection, then close',
            ],
            'mouse' => [
                'click' => 'select a table, row or cell',
                'double click' => 'edit the cell',
                'header' => 'sort by that column',
                'drag' => 'a header border resizes the column',
                'wheel' => 'scroll the focused pane',
            ],
            'commands' => [
                ':export' => 'write this table to a .sql file',
                ':sql' => 'open the SQL editor',
                ':tables' => 'focus the tables list',
                ':rows' => 'focus the rows',
                ':reload' => 'reload the table',
                ':w' => 'write marked deletions to the database',
                ':c' => 'back to the connection list',
                ':q' => 'quit',
            ],
            'from the shell' => [
                'export' => 'tql export <conn> [table] --sql=',
                'list' => 'tql export <conn> --list',
            ],
        ];
    }
}
