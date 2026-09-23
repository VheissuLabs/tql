<?php

namespace App\Tui\Islands;

use App\Keys\Keymap;
use App\Keys\Keys;

class HelpIsland extends Island
{
    public const WIDTH = 76;

    private const GAP = 3;

    private const MOST = 3;

    public string $title = 'HELP';

    public int $hidden = 0;

    public function __construct(private Styler $style, private int $offset = 0) {}

    public function widthFor(int $available): int
    {
        $columns = $this->columnsFor($available - 2);

        return min($available, $columns * $this->columnWidth() + ($columns - 1) * self::GAP + 2);
    }

    public function naturalHeight(int $innerWidth = self::WIDTH - 2): int
    {
        return count($this->lines($innerWidth));
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->lines($innerWidth);

        $this->hidden = max(0, count($lines) - $innerHeight);

        $offset = min($this->offset, $this->hidden);

        $visible = array_slice($lines, $offset, $innerHeight);

        if ($this->hidden > 0) {
            $visible[$innerHeight - 1] = ' '.$this->style->dim(
                $offset < $this->hidden ? 'j / ↓ for more' : 'g returns to the top'
            );
        }

        return $visible;
    }

    private function lines(int $width): array
    {
        $column = min($width, $this->columnWidth());
        $blocks = [];

        foreach ($this->sections() as $heading => $entries) {
            $blocks[] = $this->block($heading, $entries, $column);
        }

        $columns = array_map(
            fn (array $stack) => array_merge(...$stack),
            $this->split($blocks, $this->columnsFor($width)),
        );

        $lines = [];

        for ($row = 0; $row < max(array_map(count(...), $columns)); $row++) {
            $cells = array_map(fn (array $lines) => $this->style->pad($lines[$row] ?? '', $column), $columns);

            $lines[] = rtrim(implode(str_repeat(' ', self::GAP), $cells));
        }

        while ($lines !== [] && $lines[array_key_last($lines)] === '') {
            array_pop($lines);
        }

        return $lines;
    }

    private function block(string $heading, array $entries, int $width): array
    {
        $keys = $this->keyWidth();
        $lines = [' '.$this->style->bold($this->style->truncate($heading, $width - 1))];

        foreach ($entries as [$key, $description]) {
            $lines[] = ' '.$this->style->pad($this->style->truncate($key, $keys), $keys)
                .'  '.$this->style->dim($this->style->truncate($description, max(4, $width - $keys - 3)));
        }

        $lines[] = '';

        return $lines;
    }

    private function split(array $blocks, int $columns): array
    {
        $heights = array_map(count(...), $blocks);
        $best = [$blocks];
        $tallest = array_sum($heights);

        foreach ($this->cuts(count($blocks), min($columns, count($blocks)) - 1) as $cuts) {
            $edges = [0, ...$cuts, count($blocks)];
            $stacks = [];

            for ($at = 0; $at < count($edges) - 1; $at++) {
                $stacks[] = array_slice($blocks, $edges[$at], $edges[$at + 1] - $edges[$at]);
            }

            $height = max(array_map(fn (array $stack) => array_sum(array_map(count(...), $stack)), $stacks));

            if ($height < $tallest) {
                [$best, $tallest] = [$stacks, $height];
            }
        }

        return $best;
    }

    private function cuts(int $count, int $needed, int $from = 1): array
    {
        if ($needed === 0) {
            return [[]];
        }

        $options = [];

        for ($at = $from; $at <= $count - $needed; $at++) {
            foreach ($this->cuts($count, $needed - 1, $at + 1) as $rest) {
                $options[] = [$at, ...$rest];
            }
        }

        return $options;
    }

    private function columnsFor(int $width): int
    {
        $column = $this->columnWidth();

        return max(1, min(self::MOST, intdiv($width + self::GAP, $column + self::GAP)));
    }

    private function columnWidth(): int
    {
        $widest = 0;

        foreach ($this->sections() as $entries) {
            foreach ($entries as [, $description]) {
                $widest = max($widest, mb_strlen($description));
            }
        }

        return 1 + $this->keyWidth() + 2 + $widest;
    }

    private function keyWidth(): int
    {
        $widest = 0;

        foreach ($this->sections() as $entries) {
            foreach ($entries as [$key]) {
                $widest = max($widest, mb_strlen($key));
            }
        }

        return min(12, $widest);
    }

    private function sections(): array
    {
        return [
            'moving' => [
                [$this->keys('move_left', 'move_down', 'move_up', 'move_right'), 'move'],
                [$this->keys('next_pane', 'previous_pane'), 'next / previous pane'],
                [$this->keys('next_page', 'previous_page'), 'next / previous page'],
                [$this->keys('narrow', 'widen'), 'narrow / widen column'],
                ...$this->each('reset_width', 'sort_column', 'reload', 'redraw', 'connections'),
            ],
            'rows' => [
                ...$this->each('activate', 'inspect_row', 'view_value', 'edit_value', 'edit_row', 'new_row'),
                [$this->keys('yank_value', 'yank_row'), 'yank the value / row'],
                ...$this->each('mark_delete', 'clear_marks'),
            ],
            'finding' => $this->each(
                'filter_tables', 'filter_rows', 'structure', 'follow_link', 'jump_back',
                'databases', 'sql', 'ask', 'help', 'quit',
            ),
            'viewing a value' => [
                ['j k', 'move a line'],
                ['3j', 'move three'],
                ['g G', 'top / bottom'],
                ['12G', 'go to line 12'],
                ['V', 'select lines'],
                ['y', 'yank the selection'],
                ['ctrl+t', 'type the time'],
                ['esc', 'clear, then close'],
            ],
            'mouse' => [
                ['click', 'select'],
                ['dbl click', 'edit the cell'],
                ['header', 'sort by it'],
                ['drag edge', 'resize the column'],
                ['wheel', 'scroll'],
            ],
            'commands' => [
                [':export', 'table to .sql'],
                [':sql', 'SQL editor'],
                [':tables', 'focus the tables'],
                [':rows', 'focus the rows'],
                [':reload', 'reload'],
                [':w', 'write the changes'],
                [':c', 'connections'],
                [':q', 'quit'],
            ],
            'from the shell' => [
                ['tql export', 'table to .sql'],
                ['tql config', 'check it, --tidy'],
            ],
        ];
    }

    private function each(string ...$actions): array
    {
        $entries = [];

        foreach ($actions as $action) {
            $binding = Keymap::binding($action);

            if ($binding !== null) {
                $entries[] = [$this->keys($action), $binding->description];
            }
        }

        return $entries;
    }

    private function keys(string ...$actions): string
    {
        $glyphs = [];

        foreach ($actions as $action) {
            $binding = Keymap::binding($action);

            if ($binding !== null) {
                $glyphs[] = array_values(array_unique(array_map(Keys::glyph(...), $binding->keys)));
            }
        }

        $positions = [];

        for ($at = 0; $at < max(array_map(count(...), $glyphs ?: [[]])); $at++) {
            $parts = array_values(array_filter(array_column($glyphs, $at), fn ($glyph) => $glyph !== null));

            $tight = count($parts) >= 3 && max(array_map(mb_strlen(...), $parts)) === 1;

            $positions[] = implode($tight ? '' : ' ', $parts);
        }

        return implode(' / ', $positions);
    }
}
