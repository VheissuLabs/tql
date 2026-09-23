<?php

namespace App\Prompts\Renderers;

use App\Keys\Keymap;
use App\Support\Now;
use App\Tui\Browser;
use App\Tui\Concerns\RendersWithoutPadding;
use App\Tui\Islands\AskIsland;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\ErrorIsland;
use App\Tui\Islands\FilterIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\InspectorWidth;
use App\Tui\Islands\Island;
use App\Tui\Islands\Modal;
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
        $sidebar->title = $prompt->connection->driver === 'sqlite'
            ? $sidebar->heading()
            : trim($sidebar->heading().'  ·  '.$prompt->connection->activeDatabase());
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

            $this->modal($width, $top, $frameHeight, min($width - 4, FilterIsland::WIDTH))
                ->add($bar, $bar->rows())
                ->onto($screen);

            if ($prompt->filterForm->picker !== null) {
                $list = new PickerIsland($prompt->filterForm->picker, $style);
                $list->focused = true;

                $this->modal($width, $top, $frameHeight, min($width - 4, PickerIsland::WIDTH))
                    ->add($list, $list->rows())
                    ->onto($screen);
            }
        }

        if ($prompt->question !== null) {
            $ask = new AskIsland($prompt->question, $style, $prompt->asking);
            $ask->focused = true;

            $this->modal($width, $top, $frameHeight, min($width - 4, AskIsland::WIDTH))
                ->add($ask, AskIsland::ROWS + 5)
                ->onto($screen);
        }

        if ($prompt->databasePicker !== null) {
            $databases = new PickerIsland($prompt->databasePicker, $style);
            $databases->focused = true;

            $this->modal($width, $top, $frameHeight, min($width - 4, PickerIsland::WIDTH))
                ->add($databases, $databases->rows())
                ->onto($screen);
        }

        if ($prompt->linkPicker !== null) {
            $links = new PickerIsland($prompt->linkPicker, $style);
            $links->focused = true;

            $this->modal($width, $top, $frameHeight, min($width - 4, PickerIsland::WIDTH))
                ->add($links, $links->rows())
                ->onto($screen);
        }

        if ($prompt->mode === 'inspect' && $prompt->document !== null) {
            $document = $prompt->document;
            $selection = $prompt->documentAnchor === null ? null : $prompt->documentSelection();

            $boxWidth = max(
                InspectorWidth::MINIMUM,
                min($width - 6, $document->naturalWidth()),
            );

            // Now that the box has a width, the related rows can be laid out
            // to it: the border, and the six columns a collection is indented
            // by, are not theirs to use.
            $document->fitTo($boxWidth - 2 - 6);
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

            $modal = $this->modal($width, $top, $frameHeight, $boxWidth);

            foreach ($boxes as [$box, $height]) {
                $modal->add($box, $height);
            }

            $modal->onto($screen);
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

            $this->modal($width, $top, $frameHeight, min($width - 4, StructureIsland::WIDTH))
                ->add($structure, min($frameHeight - 2, count($prompt->columnsOf($table)) + 9))
                ->onto($screen);

            $prompt->structureHidden = $structure->hidden;
        }

        // An error sits over everything else, including whatever was open
        // when it happened.
        if ($prompt->problem !== null) {
            $problem = new ErrorIsland($prompt->problem, $style, $prompt->problemNotes, $prompt->problemOffset);
            $problem->focused = true;
            $problem->title = $prompt->problemTitle;

            $problemWidth = min($width - 4, ErrorIsland::WIDTH);

            $this->modal($width, $top, $frameHeight, $problemWidth)
                ->add($problem, min($frameHeight - 2, $problem->rows($problemWidth)))
                ->onto($screen);
        }

        if ($prompt->mode === 'help') {
            $help = new HelpIsland($style, $prompt->helpOffset);
            $help->focused = true;

            $this->modal($width, $top, $frameHeight, min($width - 4, HelpIsland::WIDTH))
                ->add($help, min($frameHeight - 2, $help->naturalHeight() + 2))
                ->onto($screen);

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
            $prompt->addedRows(),
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

        // The bar reads the keymap, so a key rebound in config.toml is the key
        // the bar offers.
        foreach (['next_pane', 'inspect_row', 'edit_value', 'sort_column', 'mark_delete', 'ask', 'filter_rows', 'structure'] as $action) {
            $this->action($action);
        }

        if ($prompt->connection->driver !== 'sqlite') {
            $this->action('databases');
        }

        $this->action('sql');

        // Paging is only worth a slot when there is somewhere to page to.
        if ($prompt->hasMore || $prompt->offset > 0) {
            $this->hotkey(Keymap::key('next_page').'/'.Keymap::key('previous_page'), 'Page');
        }

        $this->action('help');
        $this->hotkey(':'.Keymap::key('quit'), 'Quit');

        collect($this->hotkeys())
            ->map(fn (string $line) => rtrim($line))
            ->filter()
            ->each(fn (string $line) => $this->line($this->fit(' '.$line, $width)));

        $this->line($this->fit($this->status($prompt), $width));

        return $this;
    }

    /**
     * A hotkey from the keymap: its key, and the short label it carries.
     */
    private function action(string $action): void
    {
        $binding = Keymap::binding($action);

        if ($binding === null || $binding->label === null) {
            return;
        }

        $this->hotkey(Keymap::key($action), $binding->label);
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
    /**
     * A modal to hang boxes on: centred, backed and ringed for you.
     */
    private function modal(int $width, int $top, int $frameHeight, int $boxWidth): Modal
    {
        return new Modal($width, $top, $frameHeight, $boxWidth);
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
                'marked' => $this->highlight(Theme::color('deleted', 'red'), $t),
                'edited' => $this->highlight(Theme::color('edited', 'yellow'), $t),
                'added' => $this->highlight(Theme::color('added', 'green'), $t),
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

        $color = Theme::border($island->focused, $island->modal);

        $lines[] = $edge('└').$this->border($inner, $joins, '┴', $color).$edge('┘');

        return $lines;
    }

    /**
     * Inverse video swaps the foreground into the background, so setting a
     * color first is what tints the block rather than the text inside it.
     */
    private function highlight(string $color, string $text): string
    {
        return $color === 'default'
            ? $this->inverse($text)
            : $this->paint($color, $this->inverse($text));
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

    private function paint(string $color, string $text): string
    {
        return match ($color) {
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
        $color = Theme::border($island->focused, $island->modal);

        // No title, no gap to hold it: an unbroken line across the top.
        if ($island->title === '') {
            return $this->paint($color, '┌')
                .$this->run($this->borderChars($inner, $joins, '┬'), '┬', $color)
                .$this->paint($color, '┐');
        }

        $label = ' '.$island->title.' ';
        $plain = $style->visible($label);

        $label = $island->focused
            ? $this->bold($this->paint(Theme::title(true, $island->modal), $label))
            : $this->paint(Theme::title(false, $island->modal), $label);

        $edge = fn (string $text) => $this->paint($color, $text);

        $tail = array_slice(
            $this->borderChars($inner, $joins, '┬'),
            min($inner, $plain + 1),
        );

        return $edge('┌─').$label.$this->run($tail, '┬', $color).$edge('┐');
    }

    private function border(int $inner, array $joins, string $join, string $color): string
    {
        return $this->run($this->borderChars($inner, $joins, $join), $join, $color);
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
     * Paint a border row so the column ticks carry the grid color and the
     * rule between them carries the frame color, without a color code on
     * every single character.
     */
    private function run(array $chars, string $join, string $color): string
    {
        $out = '';
        $buffer = '';
        $grid = Theme::grid($this->painting);

        foreach ($chars as $char) {
            if ($char === $join) {
                $out .= ($buffer === '' ? '' : $this->paint($color, $buffer)).$this->paint($grid, $join);
                $buffer = '';

                continue;
            }

            $buffer .= $char;
        }

        return $out.($buffer === '' ? '' : $this->paint($color, $buffer));
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

            // The time shortcut goes early: the line is clamped to the
            // terminal, and a hint you cannot see is not a hint.
            return ' '.$this->bold("editing {$column}").
                $this->dim('   ↵ keeps it'.
                    ($prompt->editingTime() ? '    ctrl+t now '.Now::label() : '').
                    '    ⇧↵ adds a line    esc cancels'.
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

        $name = trim((string) $prompt->connection->color) !== ''
            ? $this->bold($this->paint((string) $prompt->connection->color, $prompt->connection->name))
            : $this->dim($prompt->connection->name);

        $tag = trim((string) $prompt->connection->tag) !== ''
            ? $this->dim(' ['.$prompt->connection->tag.']')
            : '';

        // Results replace the grid, and nothing else on screen says how to
        // get the table back.
        $back = $prompt->resultsFromQuery
            ? ' · esc goes back to '.($prompt->currentTable() ?? 'the table')
            : '';

        return ' '.$name.$tag
            .$this->dim(' · ').$this->said($prompt)
            .$this->dim($position.$back)
            .$this->link($prompt);
    }

    /**
     * What tql last said, in the color of what it is about, and lit up until
     * the next key press.
     *
     * A status line that only ever looks the same is a status line you stop
     * reading, and the whole point of it is that it changed.
     */
    private function said(Browser $prompt): string
    {
        $text = $prompt->status ?? '';

        if ($text === '') {
            return '';
        }

        $color = match (true) {
            $prompt->pendingDeletes !== [] => Theme::color('deleted', 'red'),
            $prompt->pendingEdits !== [] => Theme::color('edited', 'yellow'),
            default => Theme::title(true),
        };

        return $prompt->statusFresh
            ? $this->bold($this->paint($color, $text))
            : $this->dim($text);
    }
}
