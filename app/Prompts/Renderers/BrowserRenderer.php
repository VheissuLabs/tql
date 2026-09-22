<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\Island;
use App\Tui\Islands\Screen;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\Styler;
use App\Tui\Islands\TableIsland;
use App\Tui\Islands\ValueEditorIsland;
use App\Tui\Layout;
use App\Tui\Theme;
use Chewie\Concerns\DrawsHotkeys;
use Laravel\Prompts\Themes\Default\Renderer;

class BrowserRenderer extends Renderer
{
    use DrawsHotkeys;
    use RendersWithoutPadding;

    /** Whether the island currently being drawn has focus. */
    private bool $painting = false;

    public function __invoke(Browser $prompt): string
    {
        $width = max(60, $prompt->terminal()->cols());
        $height = max(12, $prompt->terminal()->lines());

        $top = Layout::topMargin() + 1;
        $prompt->firstBodyRow = $top + 1;
        $frameHeight = max(6, $height - $top - 3);

        $style = $this->styler();

        $sidebar = new SidebarIsland($prompt->tables, $prompt->tableIndex, $style);
        $sidebar->focused = $prompt->focus === 'sidebar';
        $sidebar->place(1, $top, Layout::sidebarWidth() + 2, $frameHeight);

        $rightX = $sidebar->x + $sidebar->width + 1;
        $rightWidth = max(20, $width - $rightX);

        $screen = (new Screen)->add($sidebar);

        $tableY = $top;
        $tableHeight = $frameHeight;

        if ($prompt->mode === 'query' || Layout::sqlAlways()) {
            $editorHeight = Layout::sqlHeight($frameHeight);
            $tableHeight = $frameHeight - $editorHeight;

            $editor = new EditorIsland(
                $prompt->editor,
                $prompt->mode === 'query',
                $prompt->lastStatement,
                $style,
            );
            $editor->focused = $prompt->mode === 'query';

            if (Layout::sqlPosition() === 'bottom') {
                $tableY = $top;
                $editor->place($rightX, $top + $tableHeight, $rightWidth, $editorHeight);
            } else {
                $tableY = $top + $editorHeight;
                $editor->place($rightX, $top, $rightWidth, $editorHeight);
            }

            $screen->add($editor);

            $prompt->editorIsland = $editor;
        }

        if ($prompt->mode === 'help') {
            $help = new HelpIsland($style);
            $help->focused = true;
            $help->place($rightX, $top, $rightWidth, $frameHeight);

            $screen->add($help);

            $tableHeight = 0;
        }

        if ($prompt->mode === 'edit' && $prompt->cellEditor !== null) {
            $editor = new ValueEditorIsland(
                $prompt->cellColumn().($prompt->editable ? '' : '  ·  read-only'),
                $prompt->cellEditor,
                $prompt->editingJson,
                $style,
                $prompt->editable,
                $prompt->visualAnchor === null ? [] : $prompt->selectedLines(),
            );
            $editor->focused = true;
            $editor->place(1, $top, $width, $frameHeight);

            $screen = (new Screen)->add($editor);

            $prompt->valueIsland = $editor;
            $tableHeight = 0;
        }

        $table = new TableIsland(
            $prompt->headers,
            $prompt->rows,
            $prompt->rowIndex,
            $prompt->columnIndex,
            $prompt->widthOverrides,
            $prompt->editing,
            $style,
            $prompt->sortColumn,
            $prompt->sortDirection,
        );
        $table->columnOffset = $prompt->columnOffset;
        $table->scrollLocked = $prompt->isDragging();
        $table->title = match (true) {
            $prompt->resultsFromQuery && $prompt->queryTable !== null => $prompt->queryTable,
            $prompt->resultsFromQuery => 'RESULTS',
            default => $prompt->currentTable() ?? 'ROWS',
        };
        $table->focused = $prompt->focus === 'grid' && $prompt->mode !== 'query';
        $table->place($rightX, $tableY, $rightWidth, max(5, $tableHeight));

        $modal = $prompt->mode === 'help' || $prompt->mode === 'edit';

        if (! $modal) {
            $screen->add($table);
        }

        collect($screen->compose($top + $frameHeight - 1, fn (Island $island) => $this->box($island, $style)))
            ->each($this->line(...));

        if ($prompt->mode !== 'query' && ! Layout::sqlAlways()) {
            $prompt->editorIsland = null;
        }

        $prompt->sidebar = $sidebar;
        $prompt->table = $table;
        $prompt->columnOffset = $table->columnOffset;
        $prompt->columnHandles = $table->handles();

        $this->clearHotkeys();
        $this->hotkey('tab', 'Pane');
        $this->hotkey('⇧tab', 'Back');
        $this->hotkey('↑↓←→', 'Move');
        $this->hotkey('s', 'SQL');
        $this->hotkey('i', 'View');
        $this->hotkey('e', 'Edit');
        $this->hotkey('< >', 'Width');
        $this->hotkey('r', 'Reload');
        $this->hotkey('n/p', 'Page');
        $this->hotkey('?', 'Help');
        $this->hotkey(':q', 'Quit');

        collect($this->hotkeys())
            ->map(fn (string $line) => rtrim($line))
            ->filter()
            ->each(fn (string $line) => $this->line(' '.$line));

        $this->line($this->status($prompt));

        return $this;
    }

    private function styler(): Styler
    {
        return new Styler(
            fn (string $t) => $this->dim($t),
            fn (string $t) => $this->bold($t),
            fn (string $t) => $this->inverse($t),
            fn (string $t) => $this->underline($t),
            fn (string $t, int $w) => $this->truncate($t, $w),
            fn (string $name, string $t) => match ($name) {
                'key' => $this->cyan($t),
                'keyword' => $this->magenta($t),
                'identifier' => $this->cyan($t),
                'operator' => $this->dim($t),
                'comment' => $this->dim($t),
                'string' => $this->green($t),
                'number' => $this->yellow($t),
                'literal' => $this->magenta($t),
                'punctuation' => $this->dim($t),
                'gutter' => $this->dim($t),
                'grid' => $this->paint(Theme::grid($this->painting), $t),
                'cursor' => $this->highlight(Theme::cursor(), $t),
                'selection' => $this->highlight(Theme::selection(), $t),
                default => $t,
            },
        );
    }

    private function box(Island $island, Styler $style): array
    {
        $this->painting = $island->focused;

        $inner = $island->innerWidth();
        $content = $island->content($inner, $island->innerHeight());
        $joins = $island->joins();

        $edge = fn (string $text) => $this->paint(Theme::border($island->focused), $text);

        $lines = [$this->topBorder($island, $style, $inner, $joins)];

        $rules = $island->ruleRows();

        for ($i = 0; $i < $island->innerHeight(); $i++) {
            $edges = in_array($i, $rules, true) ? ['├', '┤'] : ['│', '│'];

            $lines[] = $edge($edges[0]).$style->pad($content[$i] ?? '', $inner).$edge($edges[1]);
        }

        $colour = Theme::border($island->focused);

        $lines[] = $edge('└').$this->border($inner, $joins, '┴', $colour).$edge('┘');

        return $lines;
    }

    /**
     * Inverse video swaps the foreground into the background, so setting a
     * colour first is what tints the block rather than the text inside it.
     */
    private function highlight(string $colour, string $text): string
    {
        return $colour === 'default'
            ? $this->inverse($text)
            : $this->paint($colour, $this->inverse($text));
    }

    private function paint(string $colour, string $text): string
    {
        return match ($colour) {
            'default' => $text,
            'black' => $this->black($text),
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

    private function topBorder(Island $island, Styler $style, int $inner, array $joins): string
    {
        $label = ' '.$island->title.' ';
        $plain = $style->visible($label);

        $label = $island->focused
            ? $this->bold($this->paint(Theme::title(true), $label))
            : $this->paint(Theme::title(false), $label);

        $edge = fn (string $text) => $this->paint(Theme::border($island->focused), $text);

        $colour = Theme::border($island->focused);

        $tail = array_slice(
            $this->borderChars($inner, $joins, '┬'),
            min($inner, $plain + 1),
        );

        return $edge('┌─').$label.$this->run($tail, '┬', $colour).$edge('┐');
    }

    private function border(int $inner, array $joins, string $join, string $colour): string
    {
        return $this->run($this->borderChars($inner, $joins, $join), $join, $colour);
    }

    /**
     * @return array<int, string>
     */
    private function borderChars(int $inner, array $joins, string $join): array
    {
        $chars = array_fill(0, max(0, $inner), '─');

        foreach ($joins as $position) {
            if ($position >= 0 && $position < $inner) {
                $chars[$position] = $join;
            }
        }

        return $chars;
    }

    /**
     * Paint a border row so the column ticks carry the grid colour and the
     * rule between them carries the frame colour, without a colour code on
     * every single character.
     */
    private function run(array $chars, string $join, string $colour): string
    {
        $out = '';
        $buffer = '';
        $grid = Theme::grid($this->painting);

        foreach ($chars as $char) {
            if ($char === $join) {
                $out .= ($buffer === '' ? '' : $this->paint($colour, $buffer)).$this->paint($grid, $join);
                $buffer = '';

                continue;
            }

            $buffer .= $char;
        }

        return $out.($buffer === '' ? '' : $this->paint($colour, $buffer));
    }

    private function status(Browser $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.$this->paint(Theme::cursor(), '█');
        }

        if ($prompt->debugMouse && $prompt->lastMouse !== null) {
            $m = $prompt->lastMouse;

            return ' '.$this->bold('mouse').$this->dim(sprintf(
                '  raw row=%d → row=%d (offset %d)  col=%d  hit=%s  SELECTED=%s  (was %s)  rowIndex=%s',
                $m['raw_row'] ?? $m['row'], $m['row'], Layout::mouseRowOffset(), $m['column'],
                $m['sidebar'] ? 'sidebar' : ($m['table'] ? 'table' : 'nothing'),
                $m['selected'] ?? '?', $m['before'] ?? '?', $m['rowIndex'] ?? '?'
            ));
        }

        if ($prompt->mode === 'edit') {
            $column = $prompt->headers[$prompt->columnIndex] ?? '?';

            if (! $prompt->editable) {
                if ($prompt->visualAnchor !== null) {
                    [$from, $to] = $prompt->selectedLines();

                    return ' '.$this->bold('visual').
                        $this->dim('   lines '.($from + 1).'-'.($to + 1).'    y yanks    esc clears');
                }

                return ' '.$this->bold("viewing {$column}").
                    $this->dim('   j/k moves    V selects    y yanks    g/G top/bottom    esc closes');
            }

            return ' '.$this->bold("editing {$column}").
                $this->dim('   ctrl+s saves    esc cancels'.($prompt->editingJson ? '    json is validated' : ''));
        }

        if ($prompt->mode === 'help') {
            return ' '.$this->bold('help').$this->dim('   ? or esc closes');
        }

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnIndex + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position);
    }
}
