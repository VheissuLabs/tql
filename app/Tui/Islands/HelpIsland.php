<?php

namespace App\Tui\Islands;

use App\Keys\Keymap;

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
            // Read from the keymap, so a rebound key is right here without
            // anybody remembering to change two places.
            'moving' => $this->keysFor([
                'next_pane', 'previous_pane', 'move_up', 'move_left', 'next_page',
                'sort_column', 'reload', 'connections', 'yank_value', 'yank_row',
                'new_row', 'mark_delete', 'clear_marks', 'redraw',
            ]),
            'doing' => $this->keysFor([
                'activate', 'inspect_row', 'view_value', 'edit_value', 'sql', 'ask',
                'filter_tables', 'filter_rows', 'structure', 'databases',
                'follow_link', 'jump_back', 'narrow', 'reset_width', 'help', 'quit',
            ]),
            'viewing a value' => [
                'j / k' => 'move a line, 3j moves three',
                'g / G' => 'top or bottom',
                '12G' => 'jump to line 12',
                'V' => 'start a line selection',
                'y' => 'yank the selection',
                'ctrl+t' => 'type the time, in a date or datetime column',
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
                ':w' => 'write pending changes to the database',
                ':c' => 'back to the connection list',
                ':q' => 'quit',
            ],
            'from the shell' => [
                'export' => 'tql export <conn> [table] --sql=',
                'config' => 'tql config --tidy',
            ],
        ];
    }

    /**
     * @param  array<int, string>  $actions
     * @return array<string, string>
     */
    private function keysFor(array $actions): array
    {
        $lines = [];

        foreach ($actions as $action) {
            $binding = Keymap::binding($action);

            if ($binding === null) {
                continue;
            }

            $lines[$binding->shown()] = $binding->description;
        }

        return $lines;
    }
}
