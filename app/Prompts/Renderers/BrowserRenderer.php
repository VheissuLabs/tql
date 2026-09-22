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
use Chewie\Concerns\DrawsHotkeys;
use Laravel\Prompts\Themes\Default\Renderer;

class BrowserRenderer extends Renderer
{
    use DrawsHotkeys;
    use RendersWithoutPadding;

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

        if ($prompt->mode === 'query') {
            $editorHeight = min(10, max(5, intdiv($frameHeight, 3)));

            $editor = new EditorIsland($prompt->editor);
            $editor->focused = true;
            $editor->place($rightX, $top, $rightWidth, $editorHeight);

            $screen->add($editor);

            $tableY = $top + $editorHeight;
            $tableHeight = $frameHeight - $editorHeight;
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
        );
        $table->columnOffset = $prompt->columnOffset;
        $table->scrollLocked = $prompt->isDragging();
        $table->title = $prompt->resultsFromQuery ? 'RESULTS' : ($prompt->currentTable() ?? 'ROWS');
        $table->focused = $prompt->focus === 'grid' && $prompt->mode !== 'query';
        $table->place($rightX, $tableY, $rightWidth, max(5, $tableHeight));

        $modal = $prompt->mode === 'help' || $prompt->mode === 'edit';

        if (! $modal) {
            $screen->add($table);
        }

        collect($screen->compose($top + $frameHeight - 1, fn (Island $island) => $this->box($island, $style)))
            ->each($this->line(...));

        $prompt->sidebar = $sidebar;
        $prompt->table = $table;
        $prompt->columnOffset = $table->columnOffset;
        $prompt->columnHandles = $table->handles();

        $this->line($this->status($prompt));

        $this->clearHotkeys();
        $this->hotkey('tab', 'Pane');
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
                'string' => $this->green($t),
                'number' => $this->yellow($t),
                'literal' => $this->magenta($t),
                'punctuation' => $this->dim($t),
                'gutter' => $this->dim($t),
                default => $t,
            },
        );
    }

    private function box(Island $island, Styler $style): array
    {
        $inner = $island->innerWidth();
        $content = $island->content($inner, $island->innerHeight());
        $joins = $island->joins();

        $lines = [$this->topBorder($island, $style, $inner, $joins)];

        $rules = $island->ruleRows();

        for ($i = 0; $i < $island->innerHeight(); $i++) {
            $edges = in_array($i, $rules, true) ? ['├', '┤'] : ['│', '│'];

            $lines[] = $this->dim($edges[0]).$style->pad($content[$i] ?? '', $inner).$this->dim($edges[1]);
        }

        $lines[] = $this->dim('└'.$this->border($inner, $joins, '┴').'┘');

        return $lines;
    }

    private function topBorder(Island $island, Styler $style, int $inner, array $joins): string
    {
        $label = ' '.$island->title.' ';
        $plain = $style->visible($label);
        $label = $island->focused ? $this->bold($label) : $this->dim($label);

        $rule = $this->border($inner, $joins, '┬');
        $tail = mb_substr($rule, min($inner, $plain + 1));

        return $this->dim('┌─').$label.$this->dim($tail.'┐');
    }

    private function border(int $inner, array $joins, string $join): string
    {
        $chars = array_fill(0, max(0, $inner), '─');

        foreach ($joins as $position) {
            if ($position >= 0 && $position < $inner) {
                $chars[$position] = $join;
            }
        }

        return implode('', $chars);
    }

    private function status(Browser $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.'█';
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
