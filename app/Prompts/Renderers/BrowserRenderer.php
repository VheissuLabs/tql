<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\Islands\CellIsland;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\Island;
use App\Tui\Islands\Screen;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\Styler;
use App\Tui\Islands\TableIsland;
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

        if ($prompt->mode === 'inspect') {
            $cell = new CellIsland($prompt->cellColumn(), $prompt->cellText(), $style);
            $cell->focused = true;

            $cellHeight = min(12, max(5, intdiv($frameHeight, 2)));
            $cell->place($rightX, $top, $rightWidth, $cellHeight);

            $screen->add($cell);

            $tableY = $top + $cellHeight;
            $tableHeight = $frameHeight - $cellHeight;
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

        if ($prompt->mode !== 'help') {
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
        $this->hotkey('i', 'Inspect');
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
        );
    }

    private function box(Island $island, Styler $style): array
    {
        $inner = $island->innerWidth();
        $content = $island->content($inner, $island->innerHeight());

        $label = ' '.$island->title.' ';
        $label = $island->focused ? $this->bold($label) : $this->dim($label);
        $used = $style->visible($label);

        $lines = [
            $this->dim('┌─').$label.$this->dim(str_repeat('─', max(0, $inner - $used - 1)).'┐'),
        ];

        for ($i = 0; $i < $island->innerHeight(); $i++) {
            $lines[] = $this->dim('│').$style->pad($content[$i] ?? '', $inner).$this->dim('│');
        }

        $lines[] = $this->dim('└'.str_repeat('─', $inner).'┘');

        return $lines;
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

        if ($prompt->editing !== null) {
            $column = $prompt->headers[$prompt->columnIndex] ?? '?';

            return ' '.$this->bold("editing {$column}").$this->dim('   ↵ save    esc cancel');
        }

        if ($prompt->mode === 'inspect') {
            return ' '.$this->bold('inspecting '.$prompt->cellColumn()).
                $this->dim('   '.mb_strlen($prompt->cellText()).' characters    esc closes');
        }

        if ($prompt->mode === 'help') {
            return ' '.$this->bold('help').$this->dim('   ? or esc closes');
        }

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnIndex + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position);
    }
}
