<?php

namespace App\Prompts\Renderers;

use App\Tui\Browser;
use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\Islands\AskIsland;
use App\Tui\Islands\BackdropIsland;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\FilterIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\InspectorWidth;
use App\Tui\Islands\Island;
use App\Tui\Islands\PickerIsland;
use App\Tui\Islands\Screen;
use App\Tui\Islands\SectionIsland;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\StructureIsland;
use App\Tui\Islands\Styler;
use App\Tui\Islands\TableIsland;
use App\Tui\Islands\ValueEditorIsland;
use App\Tui\Layout;
use App\Tui\RowDocument;
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

        $sidebar = new SidebarIsland($prompt->visibleTables(), $prompt->tableIndex, $style, $prompt->filter);
        $sidebar->focused = $prompt->focus === 'sidebar';
        $sidebar->title = $sidebar->heading();
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

        if ($prompt->filterForm !== null) {
            $bar = new FilterIsland($prompt->filterForm, $style);
            $bar->focused = true;

            $barWidth = min($width - 4, FilterIsland::WIDTH);
            $barHeight = $bar->rows();

            $bar->place(
                (int) (($width - $barWidth) / 2) + 1,
                $top + (int) (($frameHeight - $barHeight) / 2),
                $barWidth,
                $barHeight,
            );

            $bar->modal = true;

            $screen->overlay($this->backdrop($bar, $width));
            $screen->overlay($bar);

            if ($prompt->filterForm->picker !== null) {
                $list = new PickerIsland($prompt->filterForm->picker, $style);
                $list->focused = true;

                $listWidth = min($width - 4, PickerIsland::WIDTH);
                $listHeight = $list->rows();

                $list->place(
                    (int) (($width - $listWidth) / 2) + 1,
                    $top + (int) (($frameHeight - $listHeight) / 2),
                    $listWidth,
                    $listHeight,
                );

                $list->modal = true;

                $screen->overlay($this->backdrop($list, $width));
                $screen->overlay($list);
            }
        }

        if ($prompt->question !== null) {
            $ask = new AskIsland($prompt->question, $style, $prompt->asking);
            $ask->focused = true;

            $askWidth = min($width - 4, AskIsland::WIDTH);
            $askHeight = AskIsland::ROWS + 5;

            $ask->place(
                (int) (($width - $askWidth) / 2) + 1,
                $top + (int) (($frameHeight - $askHeight) / 2),
                $askWidth,
                $askHeight,
            );

            $ask->modal = true;

            $screen->overlay($this->backdrop($ask, $width));
            $screen->overlay($ask);
        }

        if ($prompt->linkPicker !== null) {
            $links = new PickerIsland($prompt->linkPicker, $style);
            $links->focused = true;

            $linksWidth = min($width - 4, PickerIsland::WIDTH);
            $linksHeight = $links->rows();

            $links->place(
                (int) (($width - $linksWidth) / 2) + 1,
                $top + (int) (($frameHeight - $linksHeight) / 2),
                $linksWidth,
                $linksHeight,
            );

            $screen->overlay($links);
        }

        if ($prompt->mode === 'inspect' && $prompt->document !== null) {
            $document = $prompt->document;
            $selection = $prompt->documentAnchor === null ? null : $prompt->documentSelection();

            $boxWidth = min($width - 6, InspectorWidth::COLUMNS);
            $room = $frameHeight - 2;

            $boxes = [];
            $total = 0;

            foreach ([RowDocument::RECORD, RowDocument::RELATED] as $name) {
                $heading = $document->headingAt($name);

                if ($heading === null) {
                    continue;
                }

                $lines = $document->section($name);
                $folded = $document->isFolded($name);

                $box = new SectionIsland($lines, $prompt->documentLine, $style, $selection);
                $box->title = $document->lines()[$heading]['text'];
                $box->focused = $prompt->documentLine === $heading;
                $box->collapsed = $folded;

                $height = $folded ? 1 : max(3, count($lines) + 2);

                $boxes[] = [$box, $height];
                $total += $height + 1;
            }

            $total = max(0, $total - 1);

            // Share the room out when the two boxes want more than there is.
            if ($total > $room) {
                $boxes = $this->shrink($boxes, $room);
                $total = $room;
            }

            $y = $top + max(0, (int) (($frameHeight - $total) / 2));
            $x = max(1, (int) (($width - $boxWidth) / 2) + 1);

            // An opaque backdrop first, with a margin, so the panes do not
            // show through the gap between the boxes.
            $backdrop = new BackdropIsland;
            $backdrop->place(
                max(1, $x - 2),
                max($top, $y - 1),
                min($width, $boxWidth + 4),
                $total + 2,
            );

            $screen->overlay($backdrop);

            foreach ($boxes as [$box, $height]) {
                $box->modal = true;
                $box->place($x, $y, $boxWidth, $height);

                $screen->overlay($box);

                $y += $height + 1;
            }
        }

        if ($prompt->mode === 'structure') {
            $table = (string) $prompt->currentTable();

            $structure = new StructureIsland(
                $prompt->columnsOf($table),
                $prompt->links(),
                $prompt->indexesOf($table),
                $prompt->primaryKeyOf($table),
                $style,
                $prompt->structureOffset,
            );
            $structure->focused = true;
            $structure->title = 'STRUCTURE  ·  '.$table;

            $structureWidth = min($width - 4, StructureIsland::WIDTH);
            $structureHeight = min($frameHeight - 2, count($prompt->columnsOf($table)) + 9);

            $structure->place(
                (int) (($width - $structureWidth) / 2) + 1,
                $top + (int) (($frameHeight - $structureHeight) / 2),
                $structureWidth,
                $structureHeight,
            );

            $structure->modal = true;

            $screen->overlay($this->backdrop($structure, $width));
            $screen->overlay($structure);

            $prompt->structureHidden = $structure->hidden;
        }

        if ($prompt->mode === 'help') {
            $help = new HelpIsland($style, $prompt->helpOffset);
            $help->focused = true;

            $modalWidth = min($width - 4, HelpIsland::WIDTH);
            $modalHeight = min($frameHeight - 2, $help->naturalHeight() + 2);

            $help->place(
                (int) (($width - $modalWidth) / 2) + 1,
                $top + (int) (($frameHeight - $modalHeight) / 2),
                $modalWidth,
                $modalHeight,
            );

            $help->modal = true;

            $screen->overlay($this->backdrop($help, $width));
            $screen->overlay($help);

            $prompt->helpIsland = $help;
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
            $prompt->rowsWithEdits(),
            $prompt->rowIndex,
            $prompt->columnIndex,
            $prompt->widthOverrides,
            $prompt->editing,
            $style,
            $prompt->sortColumn,
            $prompt->sortDirection,
            $prompt->markedRows(),
            $prompt->editedRows(),
        );
        $table->columnOffset = $prompt->columnOffset;
        $table->scrollLocked = $prompt->isDragging();
        $filtered = $prompt->filters !== null ? ' ·  filtered' : '';

        $table->title = match (true) {
            $prompt->resultsFromQuery && $prompt->queryTable !== null => $prompt->queryTable,
            $prompt->resultsFromQuery => 'RESULTS',
            default => ($prompt->currentTable() ?? 'ROWS').$filtered,
        };
        $table->focused = $prompt->focus === 'grid' && $prompt->mode !== 'query';
        $table->place($rightX, $tableY, $rightWidth, max(5, $tableHeight));

        // Only the value editor takes the screen. Everything else floats over
        // the panes, so they stay where they were.
        $modal = $prompt->mode === 'edit';

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
        $this->hotkey('i', 'Row');
        $this->hotkey('e', 'Edit');
        $this->hotkey('o', 'Sort');
        $this->hotkey('d', 'Mark');
        $this->hotkey('a', 'Ask');
        $this->hotkey('f', 'Filter');
        $this->hotkey('t', 'Structure');
        $this->hotkey('s', 'SQL');

        // Paging is only worth a slot when there is somewhere to page to.
        if ($prompt->hasMore || $prompt->offset > 0) {
            $this->hotkey('n/p', 'Page');
        }

        $this->hotkey('?', 'Help');
        $this->hotkey(':q', 'Quit');

        collect($this->hotkeys())
            ->map(fn (string $line) => rtrim($line))
            ->filter()
            ->each(fn (string $line) => $this->line($this->fit(' '.$line, $width)));

        $this->line($this->fit($this->status($prompt), $width));

        return $this;
    }

    /**
     * Neither the hotkey bar nor the status line lives inside an island, so
     * nothing else stops them overflowing. A wrap there costs a terminal row
     * the frame does not know about, and the frame never recovers from it.
     */
    private function fit(string $line, int $width): string
    {
        return $this->visible($line) <= $width ? $line : $this->truncate($line, $width);
    }

    private function visible(string $line): int
    {
        return mb_strlen((string) preg_replace('/\e\[[0-9;]*m/', '', $line));
    }

    /**
     * An opaque margin around a modal, so the panes do not show through at
     * its edges and it reads as something on top rather than part of the grid.
     */
    private function backdrop(Island $island, int $width): BackdropIsland
    {
        $backdrop = new BackdropIsland;

        $backdrop->place(
            max(1, $island->x - 2),
            max(1, $island->y - 1),
            min($width, $island->width + 4),
            $island->height + 2,
        );

        return $backdrop;
    }

    /**
     * Give each box its share of the room, smallest first, so one long
     * relation cannot squeeze the record out of sight.
     *
     * @param  array<int, array{0: SectionIsland, 1: int}>  $boxes
     * @return array<int, array{0: SectionIsland, 1: int}>
     */
    private function shrink(array $boxes, int $room): array
    {
        $gaps = max(0, count($boxes) - 1);
        $room = max(count($boxes) * 3, $room - $gaps);

        $left = $room;
        $remaining = count($boxes);

        foreach ($boxes as $index => [$box, $height]) {
            $share = max(3, (int) ($left / $remaining));

            $boxes[$index][1] = min($height, $share);

            $left -= $boxes[$index][1];
            $remaining--;
        }

        return $boxes;
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
                'marked' => $this->highlight(Theme::colour('deleted', 'red'), $t),
                'edited' => $this->highlight(Theme::colour('edited', 'yellow'), $t),
                'selection' => $this->highlight(Theme::selection(), $t),
                default => $t,
            },
        );
    }

    private function box(Island $island, Styler $style): array
    {
        $this->painting = $island->focused;

        $inner = $island->innerWidth();

        // A collapsed section is its title bar and nothing else, so folding
        // changes the shape of the screen rather than hiding text inside a
        // box that stays the same size.
        if ($island->bare) {
            return array_map(
                fn (string $line) => $style->pad($line, $island->width),
                $island->content($island->width, $island->height),
            );
        }

        if ($island->collapsed) {
            return [$this->topBorder($island, $style, $inner, [])];
        }

        $content = $island->content($inner, $island->innerHeight());
        $joins = $island->joins();

        $edge = fn (string $text) => $this->paint(Theme::border($island->focused, $island->modal), $text);

        $lines = [$this->topBorder($island, $style, $inner, $joins)];

        $rules = $island->ruleRows();

        for ($i = 0; $i < $island->innerHeight(); $i++) {
            $edges = in_array($i, $rules, true) ? ['├', '┤'] : ['│', '│'];

            $lines[] = $edge($edges[0]).$style->pad($content[$i] ?? '', $inner).$edge($edges[1]);
        }

        $colour = Theme::border($island->focused, $island->modal);

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

    /**
     * Say what L would do, but only while the cursor is on a column where it
     * would do something. A marker in the header is noise on every row.
     */
    private function link(Browser $prompt): string
    {
        if ($prompt->mode !== 'browse') {
            return '';
        }

        $column = $prompt->headers[$prompt->columnIndex] ?? null;
        $link = $column === null ? null : ($prompt->links()[$column] ?? null);

        if ($link === null) {
            return '';
        }

        return '  '.$this->bold('L').$this->dim(' → '.$link['table']);
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
            ? $this->bold($this->paint(Theme::title(true, $island->modal), $label))
            : $this->paint(Theme::title(false, $island->modal), $label);

        $edge = fn (string $text) => $this->paint(Theme::border($island->focused, $island->modal), $text);

        $colour = Theme::border($island->focused, $island->modal);

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

        if ($prompt->filtering) {
            return ' /'.$prompt->filter.$this->paint(Theme::cursor(), '█');
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
                $this->dim('   ↵ keeps it    ⇧↵ adds a line    esc cancels'.
                    ($prompt->editingJson ? '    json is validated' : ''));
        }

        if ($prompt->mode === 'help') {
            return ' '.$this->bold('help').$this->dim('   ? or esc closes');
        }

        if ($prompt->mode === 'inspect') {
            return ' '.$this->bold('row').$this->dim(
                '   ↵ folds    j/k moves    e edits    V selects    y yanks    esc closes'
            );
        }

        $columns = count($prompt->headers);
        $position = $columns === 0 ? '' : ' · col '.($prompt->columnIndex + 1)."/{$columns}";

        return $this->dim(' '.$prompt->connection->name.' · '.($prompt->status ?? '').$position)
            .$this->link($prompt);
    }
}
