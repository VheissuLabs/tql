<?php

namespace App\Tui;

use App\Ai\Ask;
use App\Database\Filter;
use App\Database\Filters;
use App\Database\OrderBy;
use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Keys\Binding;
use App\Keys\Keymap;
use App\Keys\Keys;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Support\Now;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RedrawsOnResize;
use App\Tui\Concerns\RendersSmoothly;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\TableIsland;
use App\Tui\Islands\ValueEditorIsland;
use App\Tui\Vim\Vim;
use Chewie\Concerns\CreatesAnAltScreen;
use Chewie\Concerns\RegistersRenderers;
use Laravel\Prompts\Key;
use Laravel\Prompts\Prompt;

class Browser extends Prompt
{
    use CreatesAnAltScreen;
    use HandlesMouse;
    use RedrawsOnResize;
    use RegistersRenderers;
    use RendersSmoothly;

    /** ctrl+s: commits an edit, sends a question, applies a filter. */
    public const SAVE = "\x13";

    /**
     * Shift+enter. A plain terminal sends the same byte for enter and
     * shift+enter, so it only arrives when the terminal is told to send
     * something distinct — in Ghostty, keybind = shift+enter=csi:13;2u.
     * Alt+enter is accepted too, since it is distinct out of the box.
     */
    public const NEWLINE = ["\e[13;2u", "\e\r", "\e\n"];

    /** ctrl+o, vim's jump-back. */
    public const BACK = "\x0f";

    public const UNDO = "\x1a";

    public const REDO = "\x19";

    public const ASK = self::SAVE;

    public const PAGE = 100;

    public array $tables = [];

    public int $tableIndex = 0;

    /** Live filter on the tables list, or null when not filtering. */
    public ?string $filter = null;

    /** The question being written for the model, or null when not asking. */
    public ?QueryEditor $question = null;

    public ?string $asking = null;

    /** The filter bar while it is open, or null. */
    public ?FilterForm $filterForm = null;

    /** The filter the grid is actually showing. */
    public ?Filters $filters = null;

    /** @var array{row: int, column: int, at: float}|null */
    private ?array $lastClick = null;

    public bool $filtering = false;

    public array $headers = [];

    public array $rows = [];

    public array $raw = [];

    public int $rowIndex = 0;

    public int $columnIndex = 0;

    public int $columnOffset = 0;

    public int $visibleColumns = 1;

    public array $widthOverrides = [];

    public array $columnHandles = [];

    public ?SidebarIsland $sidebar = null;

    public ?int $sidebarWidth = null;

    public bool $tablesHidden = false;

    public ?TableIsland $table = null;

    public ?ValueEditorIsland $valueIsland = null;

    public ?EditorIsland $editorIsland = null;

    public array $inspectorBoxes = [];

    public ?HelpIsland $helpIsland = null;

    public int $helpOffset = 0;

    private ?array $drag = null;

    public int $offset = 0;

    public bool $hasMore = false;

    public ?string $lastStatement = null;

    public ?string $queryTable = null;

    /**
     * Primary key values marked for deletion, not yet written. Keyed by value
     * so a row keeps its mark across a reload or a re-sort.
     *
     * @var array<int, mixed>
     */
    public array $pendingDeletes = [];

    /**
     * Edits waiting on :w, as [primary key value => [column => value]]. Held
     * rather than written so an edit is as undoable as a mark.
     *
     * @var array<string, array<string, mixed>>
     */
    public array $pendingEdits = [];

    /**
     * Rows added but not written, each one the columns that were filled in.
     * A column nobody touched is left out, so the table's own default applies.
     *
     * @var array<int, array<string, mixed>>
     */
    public array $pendingInserts = [];

    public ?string $sortColumn = null;

    public string $sortDirection = 'asc';

    public string $mode = 'browse';

    public QueryEditor $editor;

    public string $sqlMode = 'normal';

    private ?string $lastEditKind = null;

    public Vim $vim;

    public bool $resultsFromQuery = false;

    public string $focus = 'sidebar';

    /**
     * The line under the hotkeys, and whether it has just changed.
     *
     * A property hook rather than a setter everywhere: the status is written
     * from fifty places, and every one of them should light it up for a beat
     * without having to remember to.
     */
    public ?string $status = null {
        set(?string $value) {
            $this->statusFresh = $value !== null && $value !== $this->status;

            if ($value !== $this->status) {
                $this->statusSince = microtime(true);
            }

            $this->status = $value;
        }
    }

    public float $statusSince = 0.0;

    public function fadeStatus(?float $now = null): bool
    {
        $after = Layout::statusSeconds();

        if ($after <= 0 || $this->status === null || $this->status === '') {
            return false;
        }

        if (($now ?? microtime(true)) - $this->statusSince < $after) {
            return false;
        }

        $this->status = null;

        return true;
    }

    protected function pollMicroseconds(): int
    {
        return $this->document?->waiting()
            ? 0
            : 250_000;
    }

    public function idle(bool $timedOut): bool
    {
        $redraw = $this->resized;
        $this->resized = false;

        if ($timedOut && $this->fadeStatus()) {
            $redraw = true;
        }

        return ($timedOut && $this->loadRelated()) || $redraw;
    }

    /** Set on the render right after the status changed, and not after that. */
    public bool $statusFresh = false;

    /**
     * Something that went wrong and is worth stopping for: a write the
     * database refused, a statement it could not parse. The status line is for
     * what happened; this is for what went wrong, because a reason you cannot
     * read is not a reason.
     */
    public ?string $problem = null;

    /** What to try, when there is something worth suggesting. */
    public array $problemNotes = [];

    /** How far the error has been scrolled, for one that runs long. */
    public int $problemOffset = 0;

    public ?string $command = null;

    public string $exit = 'connections';

    public ?string $editing = null;

    public ?QueryEditor $cellEditor = null;

    public bool $editingJson = false;

    public bool $editable = false;

    public ?string $readOnlyReason = null;

    public ?int $visualAnchor = null;

    public string $countPrefix = '';

    public ?int $firstBodyRow = null;

    public int $sidebarStart = 0;

    public int $gridStart = 0;

    public function __construct(
        public Connection $connection,
        private QueryRunner $runner,
        private RowFormatter $formatter,
    ) {
        $this->registerRenderer(BrowserRenderer::class);

        $this->editor = new QueryEditor(autoindent: true);
        $this->vim = new Vim;

        // A server connection with no database named opens on the list of
        // them: asking the server for tables first would answer with every
        // table in every schema on it, which is nobody's idea of a table list.
        $undecided = $connection->driver !== 'sqlite'
            && trim((string) $connection->activeDatabase()) === '';

        $this->tables = $undecided ? [] : $this->runner->tables($this->connection);

        $this->createAltScreen();

        if ($undecided) {
            $this->openDatabases();
        } elseif ($this->tables !== []) {
            $this->load();
        }

        if ($error = config('tql.config_error')) {
            $this->status = $error;
        } elseif ($notice = config('tql.config_notice')) {
            $this->status = $notice;
        }

        $this->enableMouse();

        $this->on('key', fn (string $key) => $this->onKey($key));
    }

    public function __destruct()
    {
        $this->disableMouse();

        $this->exitAltScreen();

        parent::__destruct();
    }

    public function value(): mixed
    {
        return $this->exit;
    }

    /**
     * What the screen is made of right now. When this changes, the frame is
     * repainted from scratch rather than erased by line count.
     */
    public function shape(): string
    {
        return implode('|', [
            $this->mode,
            $this->question === null ? '' : 'ask',
            $this->filterForm === null ? '' : 'filter',
            $this->filterForm?->picker === null ? '' : 'picker',
            $this->linkPicker === null ? '' : 'links',
            $this->document === null ? '' : count($this->document->lines()),
            $this->databasePicker === null ? '' : 'databases',
            $this->problem === null ? '' : 'error',
        ]);
    }

    /**
     * Stop and say what went wrong.
     *
     * @param  array<int, string>  $notes
     */
    public function fail(string $message, array $notes = [], string $title = 'ERROR'): bool
    {
        $this->problem = static::withoutPlumbing($message);
        $this->problemNotes = $notes;
        $this->problemOffset = 0;
        $this->problemTitle = $title;

        return true;
    }

    public string $problemTitle = 'ERROR';

    /**
     * A database error, with the plumbing taken out of it.
     *
     * PDO hands back the driver's message, then the connection name, the file
     * it was talking to and the statement it was running. The first part is
     * the answer; the statement is worth keeping, on its own line; the rest is
     * tql talking to itself.
     */
    private static function withoutPlumbing(string $message): string
    {
        $message = trim($message);

        $sql = null;

        if (preg_match('/,?\s*SQL:\s*(.+?)\)?$/s', $message, $match) === 1) {
            $sql = trim($match[1]);
            $message = trim(substr($message, 0, (int) mb_strpos($message, $match[0])));
        }

        $message = (string) preg_replace('/\s*\(Connection:.*$/s', '', $message);
        $message = rtrim(trim($message), ' ,(');

        return $sql === null ? $message : $message."\n\n".$sql;
    }

    private function handleErrorKey(string $key): void
    {
        if (in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true)) {
            $this->problemOffset = max(0, $this->problemOffset - 1);

            return;
        }

        if (in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true)) {
            $this->problemOffset++;

            return;
        }

        if ($key === 'y') {
            Clipboard::copy((string) $this->problem);
            $this->status = 'copied the error';

            return;
        }

        $this->problem = null;
        $this->problemNotes = [];
        $this->problemOffset = 0;
    }

    public function currentTable(): ?string
    {
        return $this->visibleTables()[$this->tableIndex] ?? null;
    }

    /**
     * The tables the sidebar is showing. Indexes everywhere are into this
     * list, so filtering does not quietly select a different table.
     *
     * @return array<int, string>
     */
    public function visibleTables(): array
    {
        if ($this->filter === null || $this->filter === '') {
            return $this->tables;
        }

        $needle = mb_strtolower($this->filter);

        return array_values(array_filter(
            $this->tables,
            fn (string $table) => str_contains(mb_strtolower($table), $needle),
        ));
    }

    public function visibleHeaders(int $count): array
    {
        return array_slice($this->headers, $this->columnOffset, $count);
    }

    public function visibleRow(array $row, int $count): array
    {
        return array_slice(array_values($row), $this->columnOffset, $count);
    }

    public function editingCell(int $rowIndex, int $absoluteColumn): bool
    {
        return $this->editing !== null
            && $this->focus === 'grid'
            && $rowIndex === $this->rowIndex
            && $absoluteColumn === $this->columnIndex;
    }

    public function onKey(string $key): void
    {
        // The highlight lasts until the next thing you do.
        $this->statusFresh = false;

        if ($event = Mouse::parse($key)) {
            // A terminal can keep reporting the mouse after we asked it to
            // stop, so honour the setting here as well as at the escape code.
            if (Layout::mouse() && $this->recordForm === null) {
                $this->onMouse($event);
            }

            return;
        }

        // An error is in front of everything, including whatever mode was
        // open when it happened: it is the only thing on screen asking to be
        // read.
        if ($this->problem !== null) {
            $this->handleErrorKey($key);

            return;
        }

        // ctrl+l, the usual way out of a terminal that has been left dirty by
        // a resize, a notification, or anything else writing over the frame.
        if ($key === "\x0c") {
            $this->repaint();
            $this->status = 'redrawn';

            return;
        }

        if ($this->recordForm !== null) {
            $this->handleRecordFormKey($key);

            return;
        }

        if ($this->palette !== null) {
            $this->handlePaletteKey($key);

            return;
        }

        if ($this->mode === 'help') {
            $this->handleHelpKey($key);

            return;
        }

        if ($this->mode === 'structure') {
            $this->handleStructureKey($key);

            return;
        }

        if ($this->mode === 'inspect') {
            $this->handleInspectKey($key);

            return;
        }

        if ($this->mode === 'query') {
            $this->handleQueryKey($key);

            return;
        }

        if ($this->mode === 'edit') {
            $this->handleEditKey($key);

            return;
        }

        if ($this->databasePicker !== null) {
            $this->handleDatabasePickerKey($key);

            return;
        }

        if ($this->linkPicker !== null) {
            $this->handleLinkPickerKey($key);

            return;
        }

        if ($this->filterForm !== null) {
            $this->handleFilterFormKey($key);

            return;
        }

        if ($this->question !== null) {
            $this->handleQuestionKey($key);

            return;
        }

        if ($this->filtering) {
            $this->handleFilterKey($key);

            return;
        }

        if ($this->command !== null) {
            $this->handleCommandKey($key);

            return;
        }

        if (ctype_digit($key) && ($key !== '0' || $this->countPrefix !== '')) {
            $this->countPrefix .= $key;

            return;
        }

        $action = (string) Keymap::action($key);
        $count = (int) $this->countPrefix;
        $this->countPrefix = '';

        if ($count > 1 && in_array($action, ['move_up', 'move_down', 'move_left', 'move_right'], true)) {
            $this->moveBy($action, $count);

            return;
        }

        $this->runAction($action);
    }

    private function moveBy(string $action, int $count): void
    {
        if ($this->focus === 'sidebar') {
            match ($action) {
                'move_up' => $this->selectTable(max(0, $this->tableIndex - $count)),
                'move_down' => $this->selectTable(min(count($this->visibleTables()) - 1, $this->tableIndex + $count)),
                default => $this->runAction($action),
            };

            return;
        }

        match ($action) {
            'move_up' => $this->rowIndex = max(0, $this->rowIndex - $count),
            'move_down' => $this->rowIndex = min(max(0, count($this->rows) - 1), $this->rowIndex + $count),
            'move_left' => $this->moveColumn(-$count),
            default => $this->moveColumn($count),
        };
    }

    private function runAction(string $action): bool
    {
        return match ($action) {
            'command' => $this->openCommandLine(),
            'quit' => $this->quit(),
            'next_pane' => $this->toggleFocus(),
            'previous_pane' => $this->toggleFocus(-1),
            'move_up' => $this->moveUp(),
            'move_down' => $this->moveDown(),
            'move_left' => match (true) {
                $this->focus === 'sidebar' => true,
                $this->columnIndex === 0 => $this->showTables(),
                default => $this->moveColumn(-1),
            },
            'move_right' => $this->focus === 'sidebar' ? $this->toggleFocus() : $this->moveColumn(1),
            'narrow' => $this->focus === 'sidebar' ? $this->resizeTables(-4) : $this->resize(-4),
            'widen' => $this->focus === 'sidebar' ? $this->resizeTables(4) : $this->resize(4),
            'reset_width' => $this->focus === 'sidebar' ? $this->resizeTables(null) : $this->resetWidth(),
            'toggle_tables' => $this->toggleTables(),
            'sql' => $this->openQuery(),
            'inspect_row' => $this->inspectRow(),
            'view_value' => $this->startEditing(readOnly: true),
            'help' => $this->toggleHelp(),
            'edit_value' => $this->startEditing(),
            'edit_row' => $this->openRecord(),
            'activate' => $this->activate(),
            'next_page' => $this->page(self::PAGE),
            'previous_page' => $this->page(-self::PAGE),
            'reload' => $this->reload(),
            'sort_column' => $this->sortBy($this->headers[$this->columnIndex] ?? null),
            'filter_tables' => $this->openFilter(),
            'ask' => $this->openQuestion(),
            'filter_rows' => $this->openFilters(),
            'structure' => $this->toggleStructure(),
            'databases' => $this->openDatabases(),
            'follow_link' => $this->followLink(),
            'jump_back' => $this->jumpBack(),
            'escape' => $this->escape(),
            'connections' => $this->quit('connections'),
            'new_row' => $this->newRow(),
            'yank_value' => $this->yankCell(),
            'yank_row' => $this->yankRow(),
            'mark_delete' => $this->markDelete(),
            'clear_marks' => $this->unmarkAll(),
            'palette' => $this->openPalette(),
            'focus_tables' => $this->showTables(),
            'focus_rows' => $this->focusPane('grid'),
            'focus_sql' => $this->openQuery(),
            default => true,
        };
    }

    public function widthFor(string $column, int $automatic): int
    {
        return $this->widthOverrides[$column] ?? $automatic;
    }

    private function resize(int $by): bool
    {
        if ($this->headers === []) {
            return true;
        }

        $column = $this->headers[$this->columnIndex] ?? null;

        if ($column === null) {
            return true;
        }

        $current = $this->widthOverrides[$column] ?? $this->naturalWidth($column);

        $this->widthOverrides[$column] = max(3, min(120, $current + $by));

        $this->status = "{$column} width {$this->widthOverrides[$column]}";

        return true;
    }

    private function resetWidth(): bool
    {
        $column = $this->headers[$this->columnIndex] ?? null;

        if ($column !== null) {
            unset($this->widthOverrides[$column]);
            $this->status = "{$column} width reset";
        }

        return true;
    }

    private function naturalWidth(string $column): int
    {
        $width = mb_strlen($column);

        foreach ($this->rows as $row) {
            $width = max($width, mb_strlen((string) ($row[$column] ?? '')));
        }

        return min($width, 28);
    }

    private function export(): bool
    {
        $exporter = app(SqlExporter::class);

        try {
            $result = $this->resultsFromQuery
                ? $exporter->rows($this->connection, 'results', $this->headers, $this->raw)
                : $exporter->table($this->connection, (string) $this->currentTable());
        } catch (\Throwable $e) {
            $this->status = 'export failed: '.$e->getMessage();

            return true;
        }

        $this->status = sprintf(
            'exported %d rows (%s) to %s',
            $result->rows,
            $result->size(),
            $result->path,
        );

        return true;
    }

    private function toggleHelp(): bool
    {
        $this->helpOffset = 0;

        $this->mode = $this->mode === 'help' ? 'browse' : 'help';

        return true;
    }

    private function handleHelpKey(string $key): void
    {
        if (in_array($key, [Key::ESCAPE, '?', 'q'], true)) {
            $this->mode = 'browse';
            $this->helpOffset = 0;

            return;
        }

        $hidden = $this->helpIsland?->hidden ?? 0;

        match (true) {
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->helpOffset = min($hidden, $this->helpOffset + 1),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->helpOffset = max(0, $this->helpOffset - 1),
            $key === 'g' => $this->helpOffset = 0,
            $key === 'G' => $this->helpOffset = $hidden,
            $key === 'n' => $this->helpOffset = min($hidden, $this->helpOffset + 10),
            $key === 'p' => $this->helpOffset = max(0, $this->helpOffset - 10),
            default => null,
        };
    }

    private function openQuery(): bool
    {
        $this->enterQueryMode();

        if ($this->editor->isEmpty() && $this->lastStatement !== null) {
            $this->editor->set($this->lastStatement);
        }

        $this->status = $this->queryHint();

        return true;
    }

    private function enterQueryMode(): void
    {
        if ($this->mode !== 'query') {
            $this->vim->reset();
            $this->sqlMode = $this->vim->mode;
        }

        $this->mode = 'query';
    }

    private function queryHint(): string
    {
        return Layout::sqlEditor() === 'vim'
            ? 'i types · :r runs it · esc returns'
            : 'ctrl+r runs it · ↵ adds a line · esc returns';
    }

    private function runKey(): string
    {
        return Layout::sqlEditor() === 'vim'
            ? ':r'
            : 'ctrl+r';
    }

    private function runOrReload(): bool
    {
        return $this->mode === 'query'
            ? $this->runFromEditor()
            : $this->reload();
    }

    private function handleQueryKey(string $key): void
    {
        if ($this->command !== null) {
            $this->handleCommandKey($key);

            return;
        }

        if (in_array(Keymap::action($key), ['palette', 'focus_tables', 'focus_rows', 'focus_sql'], true) && ! Input::isText($key)) {
            $this->runAction((string) Keymap::action($key));

            return;
        }

        if (Layout::sqlEditor() === 'vim') {
            $this->handleVimKey($key);

            return;
        }

        match (true) {
            $key === QueryEditor::RUN => $this->runStatementAtCursor(),
            $key === Key::ESCAPE => $this->leaveQuery(),
            $key === self::UNDO => $this->undoInEditor(),
            $key === self::REDO => $this->redoInEditor(),
            in_array($key, self::NEWLINE, true) => $this->editWithUndo(Key::ENTER),
            default => $this->editWithUndo($key),
        };
    }

    private function editWithUndo(string $key): void
    {
        $kind = match (true) {
            $key === Key::ENTER => 'newline',
            $key === ' ' => 'space',
            in_array($key, [Key::BACKSPACE, Key::CTRL_H, Key::DELETE], true) => 'delete',
            Input::isText($key) => 'type',
            default => 'other',
        };

        if ($kind !== $this->lastEditKind || in_array($kind, ['newline', 'space'], true)) {
            $this->editor->commit();
        }

        $this->lastEditKind = $kind === 'space'
            ? 'type'
            : $kind;

        $this->editor->checkpoint();
        $this->editor->handle($key);
    }

    private function undoInEditor(): void
    {
        $this->lastEditKind = null;

        if (! $this->editor->undo()) {
            $this->status = 'already at oldest change';
        }
    }

    private function redoInEditor(): void
    {
        $this->lastEditKind = null;

        if (! $this->editor->redo()) {
            $this->status = 'already at newest change';
        }
    }

    private function runStatementAtCursor(): void
    {
        $statement = Statements::at($this->editor->buffer(), $this->editor->cursor());

        if ($statement === null) {
            $this->runQueryBuffer();

            return;
        }

        $this->runQueryBuffer($statement['sql']);

        if ($statement['of'] > 1 && $this->resultsFromQuery && $this->problem === null) {
            $this->status .= " · statement {$statement['number']} of {$statement['of']}";
        }
    }

    private function handleVimKey(string $key): void
    {
        $outcome = $this->vim->press($this->editor, $key);

        $this->sqlMode = $this->vim->mode;

        if ($this->vim->message !== null) {
            $this->status = $this->vim->message;
        }

        match ($outcome) {
            Vim::LEAVE => $this->leaveQuery(),
            Vim::NEXT_PANE => $this->toggleFocus(),
            Vim::PREVIOUS_PANE => $this->toggleFocus(-1),
            Vim::COMMAND => $this->openCommandLine(),
            default => null,
        };
    }

    private function leaveQuery(): void
    {
        $this->editor->commit();
        $this->lastEditKind = null;
        $this->mode = 'browse';
        $this->status = null;
    }

    private function runFromEditor(): bool
    {
        $selected = $this->vim->selectedText($this->editor);

        $this->vim->leaveVisual();
        $this->sqlMode = $this->vim->mode;

        if ($selected === null) {
            $this->runStatementAtCursor();

            return true;
        }

        $this->runQueryBuffer($selected);

        return true;
    }

    private function followQueryTable(string $statement): void
    {
        $this->queryTable = null;

        if (preg_match('/\bfrom\s+[`"\[]?([A-Za-z0-9_]+)/i', $statement, $match) !== 1) {
            return;
        }

        $index = array_search($match[1], $this->tables, true);

        if ($index !== false) {
            $this->tableIndex = $index;
            $this->queryTable = $match[1];
        }
    }

    private function runQueryBuffer(?string $statement = null): void
    {
        $statement ??= $this->editor->buffer();

        if (trim($statement) === '') {
            $this->status = 'nothing to run';

            return;
        }

        $result = $this->runner->run($this->connection, $statement, 'tui');

        if ($result->failed()) {
            $this->fail((string) $result->error, [], 'THE DATABASE SAID NO');

            return;
        }

        $this->headers = $result->headers();
        $this->raw = $result->rows;
        $this->rows = $this->formatter->rows($result->rows);
        $this->rowIndex = 0;
        $this->columnIndex = 0;
        $this->columnOffset = 0;
        $this->hasMore = false;
        $this->resultsFromQuery = true;
        $this->lastStatement = $result->statement;

        // The marker follows the query. A statement you ran yourself decides
        // what is sorted, not whichever header was clicked before it.
        [$this->sortColumn, $this->sortDirection] = OrderBy::of($statement) ?? [null, 'asc'];

        $this->followQueryTable($statement);

        $this->status = "{$result->count()} rows · {$result->durationMs}ms";
    }

    private function startEditing(bool $readOnly = false): bool
    {
        if ($this->raw === []) {
            $this->status = 'nothing to open — this table has no rows';

            return true;
        }

        $this->focus = 'grid';
        $this->inspectingRow = false;
        $this->readOnlyReason = $readOnly ? 'opened with I — press e to edit' : $this->whyReadOnly();
        $this->editable = $this->readOnlyReason === null;

        $value = $this->cellValue();
        $text = $value === null ? '' : (string) $value;

        $this->editingJson = Json::looksLikeJson($text);

        $this->cellEditor = new QueryEditor;
        $this->cellEditor->set($this->editingJson ? Json::pretty($text) : $text);

        if ($this->editingJson) {
            $this->cellEditor->toStart();
        }

        $this->mode = 'edit';

        return true;
    }

    /** Set while inspecting a whole row, for the modal's title. */
    public bool $inspectingRow = false;

    public ?RowDocument $document = null;

    public ?RecordForm $recordForm = null;

    public ?Palette $palette = null;

    public int $documentLine = 0;

    public ?int $documentAnchor = null;

    public int $structureOffset = 0;

    /** Set by the renderer, so scrolling knows where the list ends. */
    public int $structureHidden = 0;

    /** @var array<int, array{table: string, filters: Filters|null, row: int}> */
    private array $jumps = [];

    /** The list open for choosing which way to follow a row. */
    public ?Picker $linkPicker = null;

    /** The list of databases on this server, while it is open. */
    public ?Picker $databasePicker = null;

    /** @var array<string, array{table: string, column: string, references: string}> */
    private array $linkChoices = [];

    public function cellColumn(): string
    {
        if ($this->inspectingRow) {
            $key = $this->keyColumn();
            $value = $key === null ? null : ($this->raw[$this->rowIndex][$key] ?? null);

            return 'row'.($value === null ? '' : ' '.$key.' '.$value);
        }

        return $this->headers[$this->columnIndex] ?? '';
    }

    /**
     * The whole row as one object, which is usually what you want when a row
     * is wider than the screen: every column on its own line, with the values
     * laid out rather than truncated into a cell.
     */
    private function inspectRow(): bool
    {
        if ($this->raw === []) {
            $this->status = 'nothing to open — this table has no rows';

            return true;
        }

        $row = $this->raw[$this->rowIndex] ?? null;

        if ($row === null) {
            return true;
        }

        $this->focus = 'grid';
        $this->inspected = [];

        $this->openInspector($this->resultsFromQuery ? null : $this->currentTable(), $row);

        $this->mode = 'inspect';

        return true;
    }

    public array $inspected = [];

    private function openInspector(?string $table, array $row): void
    {
        $this->documentLine = 1;
        $this->documentAnchor = null;
        $this->relatedRaw = [];

        $this->document = new RowDocument(
            $this->readable($row),
            $this->columnTypes($table ?? $this->currentTable()),
            $this->relatedRecords($table, $row),
        );
    }

    private function inspectRelatedRecord(RowDocument $document): void
    {
        $line = $document->lines()[$this->documentLine] ?? [];
        $row = $this->relatedRaw[$line['table'] ?? ''][$line['row'] ?? -1] ?? null;

        if ($row === null) {
            $this->status = 'i opens a related record — move onto one first';

            return;
        }

        $this->inspected[] = [
            'document' => $document,
            'line' => $this->documentLine,
            'loads' => $this->relatedLoads,
            'raw' => $this->relatedRaw,
        ];

        $this->openInspector($line['table'], $row);

        $this->status = 'inspecting '.$line['table'].' · esc goes back';
    }

    private function inspectPrevious(): void
    {
        $previous = array_pop($this->inspected);

        $this->document = $previous['document'];
        $this->documentLine = $previous['line'];
        $this->relatedLoads = $previous['loads'];
        $this->relatedRaw = $previous['raw'];
        $this->documentAnchor = null;
        $this->status = null;
    }

    private function columnTypes(?string $table): array
    {
        if ($table === null) {
            return [];
        }

        return collect($this->columnsOf($table))
            ->mapWithKeys(fn (array $column) => [(string) ($column['name'] ?? '') => (string) ($column['type_name'] ?? $column['type'] ?? '')])
            ->all();
    }

    private function relatedRecords(?string $table, array $row): array
    {
        $limit = Layout::inspectRelated();

        if ($limit < 1 || $table === null) {
            return [];
        }

        $relations = [...$this->parentsOf($table, $row), ...$this->childrenOf($table, $row, $limit)];

        $totals = $this->runner->counts($this->connection, array_map(fn (array $relation) => $relation['count'], $relations));

        $this->relatedLoads = array_map(fn (array $relation) => $relation['load'], $relations);

        return collect($relations)
            ->reject(fn (array $relation, $table) => $totals[$table] === 0)
            ->map(fn (array $relation, $table) => [
                'rows' => null,
                'shown' => min($relation['limit'], $totals[$table] ?? $relation['limit']),
                'total' => $totals[$table],
                'hide' => $relation['hide'],
                'kind' => $relation['kind'],
            ])
            ->all();
    }

    private function parentsOf(string $table, array $row): array
    {
        return collect($this->runner->foreignKeys($this->connection, $table))
            ->reject(fn (array $link, $column) => ($row[$column] ?? null) === null)
            ->mapWithKeys(fn (array $link, $column) => [$link['table'] => [
                'count' => ['table' => $link['table'], 'column' => $link['column'], 'value' => $row[$column]],
                'load' => fn () => $this->runner->related($this->connection, $link['table'], $link['column'], $row[$column], 1),
                'limit' => 1,
                'hide' => $this->idsToHide($link['table'], $link['column']),
                'kind' => 'belongs to',
            ]])
            ->all();
    }

    private function childrenOf(string $table, array $row, int $limit): array
    {
        return collect($this->runner->referencedBy($this->connection, $table))
            ->reject(fn (array $link) => ($row[$link['references']] ?? null) === null)
            ->mapWithKeys(fn (array $link) => $this->childOf($link, $row[$link['references']], $limit))
            ->all();
    }

    private function childOf(array $link, mixed $value, int $limit): array
    {
        $count = ['table' => $link['table'], 'column' => $link['column'], 'value' => $value];

        $pivot = $this->runner->pivot($this->connection, $link['table'], $link['column']);

        if ($pivot !== null) {
            return [$pivot['table'] => [
                'count' => $count,
                'load' => fn () => $this->runner->through($this->connection, $link['table'], $link['column'], $pivot, $value, $limit),
                'limit' => $limit,
                'hide' => $this->idsToHide($pivot['table'], $pivot['references']),
                'kind' => 'has many through '.$link['table'],
            ]];
        }

        return [$link['table'] => [
            'count' => $count,
            'load' => fn () => $this->runner->related($this->connection, $link['table'], $link['column'], $value, $limit),
            'limit' => $limit,
            'hide' => $this->idsToHide($link['table'], $link['column']),
            'kind' => ($link['unique'] ?? false)
                ? 'has one'
                : 'has many',
        ]];
    }

    private array $relatedLoads = [];

    private array $relatedRaw = [];

    public function loadRelated(): bool
    {
        $table = $this->mode === 'inspect'
            ? $this->document?->waiting()[0] ?? null
            : null;

        if ($table === null) {
            return false;
        }

        $this->relatedRaw[$table] = collect(($this->relatedLoads[$table] ?? fn () => [])())
            ->take(Layout::inspectRelated())
            ->values()
            ->all();

        $this->document->fill($table, array_map(fn (array $row) => $this->readable($row), $this->relatedRaw[$table]));

        return true;
    }

    /**
     * Columns of a related table that are only ids: the one joining back to
     * this row, and any foreign key pointing somewhere else. Showing them is
     * the opposite of what the inspector is for.
     *
     * @return array<int, string>
     */
    private function idsToHide(string $table, string $joinedOn): array
    {
        return array_values(array_unique(array_merge(
            [$joinedOn],
            array_keys($this->runner->foreignKeys($this->connection, $table)),
        )));
    }

    private function handleInspectKey(string $key): void
    {
        $document = $this->document;
        $lines = count($document->lines());

        if (in_array($key, [Key::ESCAPE, 'q'], true)) {
            if ($this->documentAnchor !== null) {
                $this->documentAnchor = null;
                $this->status = 'selection cleared';

                return;
            }

            if ($this->inspected !== []) {
                $this->inspectPrevious();

                return;
            }

            $this->document = null;
            $this->mode = 'browse';
            $this->status = null;

            return;
        }

        if ($key === 'e' && $this->inspected !== []) {
            $this->status = 'this record is not in the grid — open its table to edit it';

            return;
        }

        if ($key === 'e') {
            $column = $document->columnAt($this->documentLine);

            if ($column !== null) {
                $at = array_search($column, $this->headers, true);
                $this->columnIndex = $at === false ? $this->columnIndex : $at;
            }

            $this->document = null;
            $this->startEditing();

            return;
        }

        match (true) {
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->moveWithinSection($document, 1),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->moveWithinSection($document, -1),
            $key === 'g' => $this->documentLine = 0,
            $key === 'G' => $this->documentLine = $lines - 1,
            $key === Key::ENTER, $key === ' ' => $document->toggle($this->documentLine),
            $key === Key::TAB, $key === Key::SHIFT_TAB => $this->switchInspectorSection($document),
            $key === 'V' => $this->documentAnchor = $this->documentAnchor === null ? $this->documentLine : null,
            $key === 'y' => $this->yankDocument(),
            $key === 'i' => $this->inspectRelatedRecord($document),
            default => null,
        };

        $this->documentLine = min($this->documentLine, max(0, count($document->lines()) - 1));
    }

    private function moveWithinSection(RowDocument $document, int $step): void
    {
        $section = $document->lines()[$this->documentLine]['section'] ?? RowDocument::RECORD;

        $lines = [$document->headingAt($section), ...array_keys($document->section($section))];

        $this->documentLine = $lines[(array_search($this->documentLine, $lines, true) + $step + count($lines)) % count($lines)];
    }

    private function switchInspectorSection(RowDocument $document): void
    {
        $other = ($document->lines()[$this->documentLine]['section'] ?? null) === RowDocument::RECORD
            ? RowDocument::RELATED
            : RowDocument::RECORD;

        $this->documentLine = array_key_first($document->section($other))
            ?? $document->headingAt($other)
            ?? $this->documentLine;
    }

    /**
     * @return array{0: int, 1: int}
     */
    public function documentSelection(): array
    {
        $anchor = $this->documentAnchor ?? $this->documentLine;

        return [min($anchor, $this->documentLine), max($anchor, $this->documentLine)];
    }

    private function yankDocument(): void
    {
        $lines = array_column($this->document->lines(), 'text');

        [$from, $to] = $this->documentAnchor === null
            ? [0, count($lines) - 1]
            : $this->documentSelection();

        $text = implode("\n", array_slice($lines, $from, $to - $from + 1));

        $where = Clipboard::copy($text) ? 'system' : 'terminal';
        $count = $to - $from + 1;

        $this->documentAnchor = null;
        $this->status = "yanked {$count} line".($count === 1 ? '' : 's')." to the {$where} clipboard";
    }

    /**
     * A json column holds a string of json; decode it so the row reads as one
     * object rather than an object with json hiding inside it.
     *
     * @param  array<string, mixed>  $row
     * @return array<string, mixed>
     */
    private function readable(array $row): array
    {
        foreach ($row as $column => $value) {
            if (is_string($value) && Json::looksLikeJson($value)) {
                $row[$column] = json_decode($value);
            }
        }

        return $row;
    }

    private function whyReadOnly(): ?string
    {
        return match (true) {
            $this->connection->read_only => 'this connection is marked read-only',
            $this->resultsFromQuery => 'query results have no row to write back to',
            // A row that is not in the table yet is edited by position, so it
            // does not need a key to be written back by.
            $this->onAddedRow() => null,
            $this->keyColumn() === null => $this->currentTable().' has no single-column primary key',
            default => null,
        };
    }

    /**
     * Is the cursor on a row that has not been written yet?
     */
    public function onAddedRow(): bool
    {
        return $this->rowIndex < count($this->pendingInserts);
    }

    private function handleEditKey(string $key): void
    {
        // e leaves the viewer and opens the editor on the cell you were on,
        // which is what the read-only hint promises.
        if (! $this->editable && $key === 'e') {
            $this->cellEditor = null;
            $this->inspectingRow = false;

            $this->startEditing();

            return;
        }

        if (! $this->editable && ! in_array($key, [Key::ESCAPE, 'q'], true)) {
            $this->scrollOrIgnore($key);

            return;
        }

        if ($key === Key::ESCAPE && $this->visualAnchor !== null) {
            $this->visualAnchor = null;
            $this->status = 'selection cleared';

            return;
        }

        if ($key === Key::ESCAPE || (! $this->editable && $key === 'q')) {
            $wasRow = $this->inspectingRow;

            $this->cellEditor = null;
            $this->inspectingRow = false;
            $this->mode = 'browse';
            $this->status = $wasRow ? null : 'edit cancelled';

            return;
        }

        if (in_array($key, self::NEWLINE, true)) {
            $this->cellEditor?->handle(Key::ENTER);

            return;
        }

        // Enter keeps the edit, shift+enter adds a line. ctrl+s still works.
        if (in_array($key, [Key::ENTER, self::SAVE, Key::CTRL_D], true)) {
            $this->commitEdit();

            return;
        }

        // ctrl+t types the time for you, in the format this column takes.
        if ($key === "\x14") {
            $this->cellEditor?->set(Now::for($this->typeOfColumn()));
            $this->cellEditor?->toEnd();
            $this->status = 'now';

            return;
        }

        $this->cellEditor?->handle($key);
    }

    private function scrollOrIgnore(string $key): void
    {
        if (ctype_digit($key) && ! ($key === '0' && $this->countPrefix === '')) {
            $this->countPrefix .= $key;

            return;
        }

        $count = max(1, (int) ($this->countPrefix ?: 1));

        match (true) {
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->repeat($count, Key::DOWN),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->repeat($count, Key::UP),
            $key === 'g' => $this->jump(0),
            $key === 'G' => $this->jumpToCount(),
            $key === 'V' => $this->toggleVisual(),
            $key === 'y' => $this->yank(),
            default => null,
        };

        if ($key !== 'G') {
            $this->countPrefix = '';
        }
    }

    private function repeat(int $times, string $key): void
    {
        for ($i = 0; $i < $times; $i++) {
            $this->cellEditor?->handle($key);
        }
    }

    private function jump(int $line): void
    {
        $this->cellEditor?->toLine($line);
        $this->countPrefix = '';
    }

    private function jumpToCount(): void
    {
        if ($this->countPrefix === '') {
            $this->cellEditor?->toEnd();
            $this->countPrefix = '';

            return;
        }

        $this->jump(max(0, (int) $this->countPrefix - 1));
    }

    public function viewerLineAt(int $row): ?int
    {
        if ($this->valueIsland === null || $this->cellEditor === null) {
            return null;
        }

        $local = $this->valueIsland->localRow($row);

        if ($local < 0 || $local >= $this->valueIsland->innerHeight()) {
            return null;
        }

        return $this->valueIsland->lineAt($local);
    }

    private function toggleVisual(): void
    {
        $this->visualAnchor = $this->visualAnchor === null
            ? $this->cellEditor?->cursorLine()
            : null;

        $this->status = $this->visualAnchor === null
            ? 'selection cleared'
            : 'visual — move with j/k, y yanks, esc clears';
    }

    public function selectedLines(): array
    {
        if ($this->cellEditor === null) {
            return [];
        }

        $cursor = $this->cellEditor->cursorLine();

        if ($this->visualAnchor === null) {
            return [$cursor, $cursor];
        }

        return [min($this->visualAnchor, $cursor), max($this->visualAnchor, $cursor)];
    }

    private function yank(): void
    {
        if ($this->cellEditor === null) {
            return;
        }

        [$from, $to] = $this->selectedLines();

        $lines = array_slice($this->cellEditor->lines(), $from, $to - $from + 1);
        $text = implode("\n", $lines);

        $where = Clipboard::copy($text);

        $count = count($lines);

        $this->visualAnchor = null;
        $this->status = "yanked {$count} line".($count === 1 ? '' : 's')." to the {$where} clipboard";
    }

    private function commitEdit(): void
    {
        if (! $this->editable) {
            return;
        }

        $value = $this->cellEditor?->buffer() ?? '';

        if ($this->editingJson && ! Json::looksLikeJson($value)) {
            $this->status = 'not valid json — fix it or press esc to cancel';

            return;
        }

        if ($this->editingJson) {
            $value = json_encode(json_decode($value), JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        }

        // now() means the time, the way this column writes it.
        if (Now::asked($value)) {
            $value = Now::for($this->typeOfColumn());
        }

        $this->cellEditor = null;
        $this->mode = 'browse';

        $key = $this->keyColumn();
        $column = $this->headers[$this->columnIndex] ?? null;
        $row = $this->raw[$this->rowIndex] ?? null;

        if ($column === null || $row === null) {
            $this->status = 'nothing to save';

            return;
        }

        // A row that is not in the table yet has no key to edit by: the value
        // goes into the row waiting to be inserted.
        if ($this->onAddedRow()) {
            $at = $this->rowIndex;

            if ($value === '') {
                unset($this->pendingInserts[$at][$column]);
            } else {
                $this->pendingInserts[$at][$column] = $value;
            }

            $this->raw[$this->rowIndex][$column] = $value === '' ? null : $value;
            $this->rows[$this->rowIndex] = $this->addedRow($this->raw[$this->rowIndex]);

            $this->status = $this->pendingStatus();

            return;
        }

        if ($key === null) {
            $this->status = 'nothing to save';

            return;
        }

        $this->pendingEdits[(string) ($row[$key] ?? '')][$column] = $value === '' ? null : $value;

        $this->status = $this->pendingStatus();
    }

    /**
     * Rows already arrive in primary key order, so say so: the header gets its
     * marker and the SQL pane shows the order by that is really running.
     */
    private function defaultSort(): ?string
    {
        return $this->keyColumn();
    }

    /**
     * Mark or unmark the row under the cursor. Nothing is written until :w,
     * so a mis-hit costs a keystroke rather than a row.
     */
    private function yankCell(): bool
    {
        $value = $this->cellValue();

        if ($value === null && $this->raw === []) {
            return true;
        }

        return $this->yanked(
            $value === null ? 'NULL' : (string) $value,
            $this->cellColumn(),
        );
    }

    /**
     * The row as an object, ready to paste into a test or an issue.
     */
    private function yankRow(): bool
    {
        $row = $this->raw[$this->rowIndex] ?? null;

        if ($row === null) {
            return true;
        }

        return $this->yanked(
            (string) json_encode(
                $this->readable($row),
                JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
            ),
            'row',
        );
    }

    private function yanked(string $text, string $what): bool
    {
        $where = Clipboard::copy($text) ? 'system' : 'terminal';

        $this->status = "yanked {$what} to the {$where} clipboard";

        return true;
    }

    private function markDelete(): bool
    {
        if ($this->resultsFromQuery) {
            $this->status = 'query results have no row to delete — open the table itself';

            return true;
        }

        $key = $this->keyColumn();

        if ($key === null) {
            $this->status = 'no primary key on this table, so a row cannot be deleted safely';

            return true;
        }

        if ($this->focus !== 'grid') {
            $this->focus = 'grid';
        }

        $value = $this->raw[$this->rowIndex][$key] ?? null;

        if ($value === null) {
            return true;
        }

        $at = array_search($value, $this->pendingDeletes, false);

        if ($at === false) {
            $this->pendingDeletes[] = $value;
        } else {
            unset($this->pendingDeletes[$at]);
            $this->pendingDeletes = array_values($this->pendingDeletes);
        }

        // Advance the grid cursor directly. moveDown() moves whichever pane
        // has focus, which on the sidebar would change table instead.
        $this->rowIndex = min(count($this->rows) - 1, $this->rowIndex + 1);

        $this->status = $this->pendingStatus();

        return true;
    }

    private function unmarkAll(): bool
    {
        if ($this->pendingDeletes === [] && $this->pendingEdits === [] && $this->pendingInserts === []) {
            return true;
        }

        $added = $this->pendingInserts !== [];

        $this->pendingDeletes = [];
        $this->pendingEdits = [];
        $this->pendingInserts = [];

        // A row that was only ever on screen has to come off it again.
        if ($added) {
            $this->load(keepCursor: true);
        }

        $this->status = 'pending changes dropped';

        return true;
    }

    private function pendingStatus(): string
    {
        $deletes = count($this->pendingDeletes);
        $edits = count($this->pendingEdits);
        $adds = count($this->pendingInserts);

        if ($deletes === 0 && $edits === 0 && $adds === 0) {
            return 'nothing pending';
        }

        $parts = [];

        if ($adds > 0) {
            $parts[] = $adds.' row'.($adds === 1 ? '' : 's').' added';
        }

        if ($edits > 0) {
            $parts[] = $edits.' row'.($edits === 1 ? '' : 's').' edited';
        }

        if ($deletes > 0) {
            $parts[] = $deletes.' marked for deletion';
        }

        return implode(' · ', $parts).' · :w writes · u clears';
    }

    /**
     * Row indexes with an unwritten edit, for the renderer.
     *
     * @return array<int, int>
     */
    public function editedRows(): array
    {
        $key = $this->keyColumn();

        if ($key === null || $this->pendingEdits === []) {
            return [];
        }

        $edited = [];

        foreach ($this->raw as $index => $row) {
            if (isset($this->pendingEdits[(string) ($row[$key] ?? '')])) {
                $edited[] = $index;
            }
        }

        return $edited;
    }

    /**
     * The rows as they will look once written, so the grid shows what you
     * typed rather than what is still on disk.
     *
     * @return array<int, array<string, mixed>>
     */
    public function rowsWithEdits(): array
    {
        $key = $this->keyColumn();

        if ($key === null || $this->pendingEdits === []) {
            return $this->rows;
        }

        $rows = $this->rows;

        foreach ($this->raw as $index => $row) {
            $edits = $this->pendingEdits[(string) ($row[$key] ?? '')] ?? null;

            if ($edits === null) {
                continue;
            }

            foreach ($edits as $column => $value) {
                $rows[$index][$column] = $this->formatter->rows([[$column => $value]])[0][$column];
            }
        }

        return $rows;
    }

    private function newRow(): bool
    {
        if ($this->resultsFromQuery) {
            $this->status = 'query results have no table to add to';

            return true;
        }

        if ($this->currentTable() === null || $this->headers === []) {
            $this->status = 'no table to add to';

            return true;
        }

        if ($this->connection->read_only) {
            $this->status = 'this connection is marked read-only';

            return true;
        }

        $table = (string) $this->currentTable();

        $this->recordForm = RecordForm::adding(
            $table,
            $this->columnsOf($table),
            $this->connection->driver,
            $this->generatedColumns(),
            $this->suggestKey($this->requiredColumns()),
        );

        $this->status = null;

        return true;
    }

    private function openRecord(): bool
    {
        if ($this->raw === []) {
            $this->status = 'nothing to open — this table has no rows';

            return true;
        }

        $reason = $this->whyReadOnly();

        if ($reason !== null) {
            $this->status = $reason;

            return true;
        }

        $table = (string) $this->currentTable();
        $columns = $this->columnsOf($table);

        if ($this->onAddedRow()) {
            $this->recordForm = RecordForm::pending(
                $table,
                $columns,
                $this->connection->driver,
                $this->generatedColumns(),
                $this->pendingInserts[$this->rowIndex],
                $this->rowIndex,
            );
        } else {
            $key = (string) $this->keyColumn();
            $row = $this->raw[$this->rowIndex];

            $this->recordForm = RecordForm::editing(
                $table,
                $columns,
                array_merge($row, $this->pendingEdits[(string) ($row[$key] ?? '')] ?? []),
                $key,
            );
        }

        $at = array_search($this->headers[$this->columnIndex] ?? null, array_column($this->recordForm->fields(), 'name'), true);
        $this->recordForm->jump($at === false ? 0 : $at);

        $this->focus = 'grid';
        $this->status = null;

        return true;
    }

    private function generatedColumns(): array
    {
        return array_values(array_filter($this->headers, $this->filledByDatabase(...)));
    }

    private function handleRecordFormKey(string $key): void
    {
        $form = $this->recordForm;

        if ($form->editor !== null) {
            $this->handleRecordFieldKey($form, $key);

            return;
        }

        if ($key !== Key::ESCAPE) {
            $form->discarding = false;
        }

        match (true) {
            $key === Key::ESCAPE => $this->closeRecord(),
            $key === self::SAVE => $this->saveRecord(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k', Key::SHIFT_TAB], true) => $form->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j', Key::TAB], true) => $form->move(1),
            $key === 'g' => $form->jump(0),
            $key === 'G' => $form->jump(PHP_INT_MAX),
            in_array($key, [Key::ENTER, 'e'], true) => $form->start(),
            $key === Key::CTRL_N => $form->setNull(),
            in_array($key, [Key::BACKSPACE, Key::CTRL_H, Key::DELETE], true) => $form->reset(),
            default => null,
        };
    }

    private function handleRecordFieldKey(RecordForm $form, string $key): void
    {
        if ($key === Key::ESCAPE) {
            $form->abandon();

            return;
        }

        if ($key === "\x14") {
            $form->now();

            return;
        }

        if ($key === self::SAVE) {
            $this->saveRecord();

            return;
        }

        if (in_array($key, self::NEWLINE, true)) {
            $form->expanded = true;
            $form->type(Key::ENTER);

            return;
        }

        $inline = ! $form->expanded;

        $step = match (true) {
            in_array($key, [Key::ENTER, Key::TAB, Key::CTRL_D], true) => 1,
            $key === Key::SHIFT_TAB => -1,
            $inline && in_array($key, [Key::DOWN, Key::DOWN_ARROW], true) => 1,
            $inline && in_array($key, [Key::UP, Key::UP_ARROW], true) => -1,
            default => null,
        };

        if ($step === null) {
            $form->type($key);

            return;
        }

        if ($form->keep()) {
            $form->move($step);
        }
    }

    private function closeRecord(): void
    {
        $form = $this->recordForm;

        if ($form->dirty() && ! $form->discarding) {
            $form->discarding = true;

            return;
        }

        $this->recordForm = null;
        $this->status = $form->dirty() ? 'row thrown away' : null;
    }

    private function saveRecord(): void
    {
        $form = $this->recordForm;

        if (! $form->keep()) {
            return;
        }

        $values = $form->values();
        $this->recordForm = null;
        $this->focus = 'grid';

        if ($form->insertAt !== null) {
            $this->pendingInserts[$form->insertAt] = $values;
            $this->load(keepCursor: true);
        } elseif ($form->adds()) {
            array_unshift($this->pendingInserts, $values);
            $this->load(keepCursor: true);
            $this->rowIndex = 0;
        } elseif ($values === []) {
            $this->status = 'nothing changed';

            return;
        } else {
            foreach ($values as $column => $value) {
                $this->pendingEdits[(string) $form->keyValue][$column] = $value;
            }
        }

        $this->status = $this->pendingStatus();
    }

    /**
     * A key the database is not going to give out gets the next one going.
     *
     * Most primary keys are generated and this never happens. The ones that
     * are not — a schema converted from somewhere that lost its auto
     * increment, a table keyed by hand — leave you typing a number you have to
     * go and look up, so tql looks it up.
     *
     * @param  array<int, string>  $required
     * @return array<string, string> the column, and what it was filled with
     */
    private function suggestKey(array $required): array
    {
        $key = $this->keyColumn();
        $table = $this->currentTable();

        if ($key === null || $table === null || ! in_array($key, $required, true)) {
            return [];
        }

        if (! $this->numeric($key)) {
            return [];
        }

        $grammar = $this->runner->grammarFor($this->connection);

        $result = $this->runner->run(
            $this->connection,
            'select max('.$grammar->wrap($key).') as highest from '.$grammar->wrapTable($table),
            'tui',
        );

        if ($result->failed()) {
            return [];
        }

        return [$key => (string) ((int) ($result->rows[0]['highest'] ?? 0) + 1)];
    }

    /**
     * Does this column hold numbers? Only then is "the next one" a number.
     */
    private function numeric(string $column): bool
    {
        $table = $this->currentTable();

        foreach ($table === null ? [] : $this->columnsOf($table) as $candidate) {
            if (($candidate['name'] ?? null) !== $column) {
                continue;
            }

            $type = strtolower((string) ($candidate['type_name'] ?? ''));

            foreach (['int', 'serial', 'numeric', 'decimal'] as $shape) {
                if (str_contains($type, $shape)) {
                    return true;
                }
            }
        }

        return false;
    }

    /**
     * Columns with nothing to fall back on: not nullable, no default, and not
     * something the database generates.
     *
     * @return array<int, string>
     */
    public function requiredColumns(): array
    {
        $table = $this->currentTable();

        if ($table === null) {
            return [];
        }

        $required = [];

        foreach ($this->columnsOf($table) as $column) {
            $name = (string) ($column['name'] ?? '');

            if ($name === '' || $this->filledByDatabase($name)) {
                continue;
            }

            $nullable = (bool) ($column['nullable'] ?? false);
            $default = $column['default'] ?? null;

            if (! $nullable && ($default === null || $default === '')) {
                $required[] = $name;
            }
        }

        return $required;
    }

    /**
     * Will the database put something there without being asked?
     */
    private function filledByDatabase(string $column): bool
    {
        $table = $this->currentTable();

        if ($table === null) {
            return false;
        }

        foreach ($this->columnsOf($table) as $candidate) {
            if (($candidate['name'] ?? null) !== $column) {
                continue;
            }

            if ((bool) ($candidate['auto_increment'] ?? false)) {
                return true;
            }

            // In SQLite an integer primary key is the rowid, which is given
            // out whether or not the schema says auto increment.
            return $this->connection->driver === 'sqlite'
                && $column === $this->keyColumn()
                && strtolower((string) ($candidate['type_name'] ?? '')) === 'integer';
        }

        return false;
    }

    /**
     * Put the unwritten rows back on top of the grid, after a load.
     */
    private function appendPendingRows(): void
    {
        foreach (array_reverse($this->pendingInserts) as $values) {
            $row = array_fill_keys($this->headers, null);

            foreach ($values as $column => $value) {
                $row[$column] = $value;
            }

            $display = $this->addedRow($row);

            foreach (array_keys(array_filter($values, fn (mixed $value) => $value === null)) as $column) {
                $display[$column] = $this->formatter->rows([[$column => null]])[0][$column];
            }

            array_unshift($this->raw, $row);
            array_unshift($this->rows, $display);
        }
    }

    /**
     * A row waiting to be written, as the grid shows it.
     *
     * Nothing is NULL here yet: a column nobody filled in is blank, because
     * what it ends up holding is the table's business, not something tql can
     * claim to know.
     *
     * @param  array<string, mixed>  $row
     * @return array<string, string>
     */
    private function addedRow(array $row): array
    {
        $display = $this->formatter->rows([$row])[0];

        foreach ($row as $column => $value) {
            if ($value === null) {
                $display[$column] = '';
            }
        }

        return $display;
    }

    /**
     * Row indexes that are not in the table yet, for the renderer. They are
     * the first rows in the grid, newest first.
     *
     * @return array<int, int>
     */
    public function addedRows(): array
    {
        return $this->pendingInserts === []
            ? []
            : range(0, count($this->pendingInserts) - 1);
    }

    /**
     * Row indexes currently marked, for the renderer.
     *
     * @return array<int, int>
     */
    public function markedRows(): array
    {
        $key = $this->keyColumn();

        if ($key === null || $this->pendingDeletes === []) {
            return [];
        }

        $marked = [];

        foreach ($this->raw as $index => $row) {
            if (in_array($row[$key] ?? null, $this->pendingDeletes, false)) {
                $marked[] = $index;
            }
        }

        return $marked;
    }

    public function writePending(): bool
    {
        if ($this->pendingDeletes === [] && $this->pendingEdits === [] && $this->pendingInserts === []) {
            $this->status = 'nothing to write';

            return true;
        }

        $table = $this->currentTable();
        $key = $this->keyColumn();

        if ($table === null) {
            return true;
        }

        if ($key === null && ($this->pendingEdits !== [] || $this->pendingDeletes !== [])) {
            return true;
        }

        $edits = count($this->pendingEdits);
        $deletes = count($this->pendingDeletes);
        $adds = count($this->pendingInserts);

        foreach ($this->pendingInserts as $values) {
            $result = $this->runner->insert($this->connection, $table, $values);

            if ($result->failed()) {
                return $this->fail($result->error, [
                    'The row is still here — fix it and :w again, or u to drop it.',
                ], 'COULD NOT ADD THE ROW');
            }
        }

        foreach ($this->pendingEdits as $keyValue => $columns) {
            foreach ($columns as $column => $value) {
                $result = $this->runner->update(
                    $this->connection,
                    $table,
                    $column,
                    $key,
                    $keyValue,
                    $value,
                );

                if ($result->failed()) {
                    return $this->fail($result->error, [
                        'The change is still here — fix it and :w again, or u to drop it.',
                    ], 'COULD NOT WRITE');
                }
            }
        }

        if ($deletes > 0) {
            $result = $this->runner->delete($this->connection, $table, $key, $this->pendingDeletes);

            if ($result->failed()) {
                return $this->fail($result->error, [
                    'The rows are still marked — u clears them.',
                ], 'COULD NOT DELETE');
            }
        }

        $this->pendingDeletes = [];
        $this->pendingEdits = [];
        $this->pendingInserts = [];

        $this->load(keepCursor: true);

        $written = [];

        if ($adds > 0) {
            $written[] = $adds.' row'.($adds === 1 ? '' : 's').' added';
        }

        if ($edits > 0) {
            $written[] = $edits.' row'.($edits === 1 ? '' : 's').' updated';
        }

        if ($deletes > 0) {
            $written[] = $deletes.' deleted';
        }

        $this->status = 'wrote '.implode(' · ', $written);

        return true;
    }

    private function keyColumn(): ?string
    {
        $table = $this->currentTable();

        return $table === null ? null : $this->runner->primaryKey($this->connection, $table);
    }

    private function cellValue(): mixed
    {
        $row = $this->raw[$this->rowIndex] ?? null;
        $column = $this->headers[$this->columnIndex] ?? null;

        return $row === null || $column === null ? null : ($row[$column] ?? null);
    }

    private function onMouse(array $event): void
    {
        if ($this->mode === 'edit') {
            $this->mouseInViewer($event);

            return;
        }

        if ($this->editing !== null) {
            return;
        }

        if (! $event['pressed']) {
            $this->drag = null;

            return;
        }

        if ($event['button'] === Mouse::DRAG_LEFT) {
            $this->dragTo($event['column']);

            return;
        }

        match ($event['button']) {
            Mouse::WHEEL_UP => $this->moveUp(),
            Mouse::WHEEL_DOWN => $this->moveDown(),
            Mouse::LEFT => $this->press($event['column'], $event['row']),
            default => true,
        };
    }

    private function mouseInViewer(array $event): void
    {
        if ($this->editable || ! $event['pressed']) {
            return;
        }

        if ($event['button'] === Mouse::WHEEL_UP) {
            $this->cellEditor?->handle(Key::UP);

            return;
        }

        if ($event['button'] === Mouse::WHEEL_DOWN) {
            $this->cellEditor?->handle(Key::DOWN);

            return;
        }

        $line = $this->viewerLineAt($event['row']);

        if ($line === null) {
            return;
        }

        if ($event['button'] === Mouse::DRAG_LEFT) {
            $this->visualAnchor ??= $this->cellEditor?->cursorLine();
            $this->jump($line);

            return;
        }

        if ($event['button'] === Mouse::LEFT) {
            $this->visualAnchor = null;
            $this->jump($line);
        }
    }

    private function press(int $column, int $row): bool
    {
        $this->focusPaneUnder($column, $row);

        if ($this->sidebar !== null
            && $column === $this->sidebar->x + $this->sidebar->width - 1
            && $row >= $this->sidebar->y
            && $row <= $this->sidebar->y + $this->sidebar->height - 1) {
            $this->drag = ['tables' => true];

            return true;
        }

        if ($this->table !== null
            && $row === $this->table->y + 1
            && $column >= $this->table->x
            && $column <= $this->table->x + $this->table->width - 1) {
            $handle = $this->handleNear($column);

            if ($handle !== null) {
                $name = $this->headers[$handle] ?? null;

                if ($name !== null) {
                    $this->drag = ['column' => $name, 'index' => $handle];

                    $this->columnIndex = $handle;

                    return true;
                }
            }

            return $this->sortFromClick($column);
        }

        return $this->click($column, $row);
    }

    private function focusPaneUnder(int $column, int $row): void
    {
        $pane = match (true) {
            $this->sidebar?->contains($column, $row) === true => 'sidebar',
            $this->table?->contains($column, $row) === true => 'grid',
            default => null,
        };

        if ($pane === null) {
            return;
        }

        if ($this->mode === 'query') {
            $this->mode = 'browse';
            $this->status = null;
        }

        $this->focus = $pane;
    }

    public function isDragging(): bool
    {
        return $this->drag !== null;
    }

    private function handleNear(int $column): ?int
    {
        foreach ($this->columnHandles as $index => $x) {
            if (abs($column - $x) <= 1) {
                return $index;
            }
        }

        return null;
    }

    private function dragTo(int $column): void
    {
        if (($this->drag['tables'] ?? false) === true) {
            $this->sidebarWidth = max(8, min(80, $column - 2));
            $this->status = 'table list width '.$this->sidebarWidth;

            return;
        }

        if ($this->drag === null || $this->table === null) {
            return;
        }

        $start = $this->table->cellStart($this->drag['index']);

        if ($start === null) {
            return;
        }

        $width = max(3, min(200, $column - $start - 2));

        $this->widthOverrides[$this->drag['column']] = $width;

        $this->status = "{$this->drag['column']} width {$width}";
    }

    private function click(int $column, int $row): bool
    {
        if ($this->sidebar !== null && $this->sidebar->containsContent($column, $row)) {
            return $this->clickSidebar($this->sidebar->localRow($row));
        }

        if ($this->table !== null && $this->table->containsContent($column, $row)) {
            return $this->clickTable($this->table->localRow($row), $this->table->localColumn($column));
        }

        if ($this->editorIsland !== null && $this->editorIsland->contains($column, $row)) {
            return $this->clickEditor(
                $this->editorIsland->localRow($row),
                $this->editorIsland->localColumn($column),
            );
        }

        return true;
    }

    /**
     * The border and the title count as the pane, so a click near its edge
     * focuses it rather than doing nothing.
     */
    private function clickEditor(int $localRow, int $localColumn): bool
    {
        $entering = $this->mode !== 'query';

        $this->enterQueryMode();
        $this->status = $this->queryHint();

        if (! $entering || $localRow >= 0) {
            $this->editor->toLineColumn(...$this->editorIsland->positionAt($localRow, $localColumn));
        }

        return true;
    }

    private function clickSidebar(int $localRow): bool
    {
        $target = $this->sidebar?->selectedIndexFor($localRow);

        if ($target === null) {
            return true;
        }

        $this->selectTable($target);

        return true;
    }

    private function clickTable(int $localRow, int $localColumn): bool
    {
        $rowIndex = $this->table?->rowIndexFor($localRow);

        if ($rowIndex === null) {
            return true;
        }

        $this->focus = 'grid';
        $this->rowIndex = $rowIndex;

        $columnIndex = $this->table?->columnIndexFor($localColumn);

        if ($columnIndex !== null) {
            $this->columnIndex = $columnIndex;
        }

        if ($this->isDoubleClick($rowIndex, $this->columnIndex)) {
            return $this->startEditing();
        }

        return true;
    }

    /**
     * A second click on the same cell, soon enough. Terminals report two plain
     * presses rather than a double-click event, so the timing is ours to keep.
     */
    private function isDoubleClick(int $row, int $column): bool
    {
        $now = microtime(true);
        $last = $this->lastClick;

        $this->lastClick = ['row' => $row, 'column' => $column, 'at' => $now];

        if ($last === null || $last['row'] !== $row || $last['column'] !== $column) {
            return false;
        }

        if (($now - $last['at']) * 1000 > Layout::doubleClickMs()) {
            return false;
        }

        // Clear it, so three clicks are not two double clicks.
        $this->lastClick = null;

        return true;
    }

    public function sortBy(?string $column): bool
    {
        if ($column === null) {
            return true;
        }

        if ($this->resultsFromQuery) {
            return $this->sortQueryResults($column);
        }

        if ($this->sortColumn === $column) {
            if ($this->sortDirection === 'asc') {
                $this->sortDirection = 'desc';
            } else {
                $this->sortColumn = $this->defaultSort();
                $this->sortDirection = 'asc';
            }
        } else {
            $this->sortColumn = $column;
            $this->sortDirection = 'asc';
        }

        $this->offset = 0;
        $this->load(keepCursor: true);

        $this->status = $this->sortColumn === null
            ? 'sort cleared'
            : "sorted by {$this->sortColumn} {$this->sortDirection}";

        return true;
    }

    /**
     * Results the user ran themselves are sorted by rewriting their own
     * statement, so the SQL pane keeps showing the query that produced what
     * is on screen rather than quietly diverging from it.
     */
    private function sortQueryResults(string $column): bool
    {
        $direction = match (true) {
            $this->sortColumn !== $column => 'asc',
            $this->sortDirection === 'asc' => 'desc',
            default => null,
        };

        $grammar = $this->runner->grammarFor($this->connection);

        $rewritten = OrderBy::apply(
            $this->editor->buffer(),
            $direction === null ? null : $column,
            $direction ?? 'asc',
            $grammar->wrap($column),
        );

        if ($rewritten === null) {
            $this->status = 'this query is too complex to sort — add an order by yourself';

            return true;
        }

        $this->sortColumn = $direction === null ? null : $column;
        $this->sortDirection = $direction ?? 'asc';

        $this->editor->set($rewritten);
        $this->runQueryBuffer();

        return true;
    }

    private function sortFromClick(int $column): bool
    {
        $index = $this->table?->columnIndexFor($this->table->localColumn($column));

        return $this->sortBy($index === null ? null : ($this->headers[$index] ?? null));
    }

    private function quit(string $exit = 'quit'): bool
    {
        if (($this->pendingDeletes !== [] || $this->pendingEdits !== []) && $exit !== 'forced') {
            $count = count($this->pendingDeletes) + count($this->pendingEdits);

            $this->pendingDeletes = [];
            $this->pendingEdits = [];
            $this->status = $count.' unwritten change'.($count === 1 ? '' : 's').
                ' dropped — :q again to quit';

            return true;
        }

        $this->exit = $exit;
        $this->state = 'submit';

        return false;
    }

    /**
     * A server holds many databases. Switching is a reconnection, not a
     * query, so everything on screen is rebuilt from the new one.
     */
    public function openDatabases(): bool
    {
        if ($this->connection->driver === 'sqlite') {
            $this->status = 'a sqlite connection is one file';

            return true;
        }

        $databases = $this->runner->databases($this->connection);

        if ($databases === []) {
            $this->status = 'no databases to switch to';

            return true;
        }

        $this->databasePicker = new Picker(
            'DATABASE',
            $databases,
            (string) $this->connection->activeDatabase(),
        );

        return true;
    }

    private function handleDatabasePickerKey(string $key): void
    {
        $picker = $this->databasePicker;

        match (true) {
            $key === Key::ESCAPE, $key === 'q' => $this->databasePicker = null,
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $picker->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $picker->move(1),
            $key === Key::ENTER => $this->useDatabase($picker->selected()),
            default => $picker->type($key),
        };
    }

    private function useDatabase(?string $chosen): void
    {
        $this->databasePicker = null;

        if ($chosen === null || $chosen === $this->connection->activeDatabase()) {
            return;
        }

        // Held on the instance, not the record: opening a connection saves it
        // to record last used, and that must not persist a session choice.
        $this->connection->sessionDatabase = $chosen;

        $this->jumps = [];
        $this->filters = null;
        $this->filter = null;
        $this->sortColumn = null;
        $this->offset = 0;
        $this->pendingDeletes = [];
        $this->pendingEdits = [];

        $this->tables = $this->runner->tables($this->connection);
        $this->tableIndex = 0;

        if ($this->tables === []) {
            $this->headers = [];
            $this->rows = [];
            $this->raw = [];
            $this->status = $chosen.' has no tables';

            return;
        }

        $this->load();

        $this->status = 'using '.$chosen;
    }

    private function toggleStructure(): bool
    {
        if ($this->currentTable() === null) {
            return true;
        }

        $this->structureOffset = 0;
        $this->mode = $this->mode === 'structure' ? 'browse' : 'structure';

        return true;
    }

    private function handleStructureKey(string $key): void
    {
        if (in_array($key, [Key::ESCAPE, 'q', 't'], true)) {
            $this->mode = 'browse';
            $this->structureOffset = 0;

            return;
        }

        $hidden = $this->structureHidden;

        match (true) {
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->structureOffset = min($hidden, $this->structureOffset + 1),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->structureOffset = max(0, $this->structureOffset - 1),
            $key === 'g' => $this->structureOffset = 0,
            $key === 'G' => $this->structureOffset = $hidden,
            default => null,
        };
    }

    /**
     * @return array<string, array{table: string, column: string}>
     */
    /**
     * @return array<int, array<string, mixed>>
     */
    /**
     * Is the value being edited a date, a time or a timestamp?
     */
    public function editingTime(): bool
    {
        return Now::suits($this->typeOfColumn());
    }

    /**
     * The declared type of the column the cursor is on, for anything that has
     * to write a value the way that column takes it.
     */
    private function typeOfColumn(): ?string
    {
        $table = $this->currentTable();
        $column = $this->headers[$this->columnIndex] ?? null;

        if ($table === null || $column === null) {
            return null;
        }

        foreach ($this->columnsOf($table) as $candidate) {
            if (($candidate['name'] ?? null) === $column) {
                return (string) ($candidate['type_name'] ?? $candidate['type'] ?? '');
            }
        }

        return null;
    }

    public function columnsOf(string $table): array
    {
        return array_map(fn ($column) => (array) $column, $this->runner->columns($this->connection, $table));
    }

    /**
     * @return array<int, array<string, mixed>>
     */
    public function indexesOf(string $table): array
    {
        return $this->runner->indexes($this->connection, $table);
    }

    public function primaryKeyOf(string $table): ?string
    {
        return $this->runner->primaryKey($this->connection, $table);
    }

    public function links(): array
    {
        $table = $this->currentTable();

        if ($table === null || $this->resultsFromQuery) {
            return [];
        }

        return $this->runner->foreignKeys($this->connection, $table);
    }

    /**
     * Follow the foreign key under the cursor: open the table it points at,
     * filtered to the row it points to. The filter is a real where clause, so
     * the SQL pane shows how the jump was made.
     */
    /**
     * @return array<int, array{table: string, column: string, references: string}>
     */
    public function backLinks(): array
    {
        $table = $this->currentTable();

        if ($table === null || $this->resultsFromQuery) {
            return [];
        }

        return $this->runner->referencedBy($this->connection, $table);
    }

    private function followLink(): bool
    {
        $column = $this->headers[$this->columnIndex] ?? null;
        $link = $this->links()[$column] ?? null;

        // Nothing to follow forwards, so offer what points back at this row.
        if ($link === null) {
            return $this->followBack();
        }

        $value = $this->raw[$this->rowIndex][$column] ?? null;

        if ($value === null) {
            $this->status = $column.' is empty on this row';

            return true;
        }

        if (! in_array($link['table'], $this->tables, true)) {
            $this->status = $link['table'].' is not in this database';

            return true;
        }

        $this->jumps[] = [
            'table' => (string) $this->currentTable(),
            'filters' => $this->filters,
            'row' => $this->rowIndex,
        ];

        $this->openLinked($link['table'], new Filters([new Filter($link['column'], 'is', (string) $value)]));

        $this->status = 'followed '.$column.' → '.$link['table'].'  ·  esc goes back';

        return true;
    }

    /**
     * From a row, open a table that references it: an artist to their albums.
     * One candidate goes straight there, several open a list to pick from.
     */
    private function followBack(): bool
    {
        $back = $this->backLinks();

        if ($back === []) {
            $column = $this->headers[$this->columnIndex] ?? null;

            $this->status = $column === null
                ? 'nothing to follow'
                : 'nothing links to '.$this->currentTable().', and '.$column.' is not a foreign key';

            return true;
        }

        $this->linkChoices = [];

        foreach ($back as $link) {
            $this->linkChoices[$link['table'].'.'.$link['column']] = $link;
        }

        if (count($back) === 1) {
            return $this->openBackLink($back[0]);
        }

        $this->linkPicker = new Picker('REFERENCED BY', array_keys($this->linkChoices));

        return true;
    }

    /**
     * @param  array{table: string, column: string, references: string}  $link
     */
    private function openBackLink(array $link): bool
    {
        $value = $this->raw[$this->rowIndex][$link['references']] ?? null;

        if ($value === null) {
            $this->status = $link['references'].' is empty on this row';

            return true;
        }

        if (! in_array($link['table'], $this->tables, true)) {
            return true;
        }

        $this->jumps[] = [
            'table' => (string) $this->currentTable(),
            'filters' => $this->filters,
            'row' => $this->rowIndex,
        ];

        $this->openLinked($link['table'], new Filters([new Filter($link['column'], 'is', (string) $value)]));

        $this->status = 'followed → '.$link['table'].' where '.$link['column'].' is '.$value.
            '  ·  esc goes back';

        return true;
    }

    private function handleLinkPickerKey(string $key): void
    {
        $picker = $this->linkPicker;

        match (true) {
            $key === Key::ESCAPE, $key === 'q' => $this->linkPicker = null,
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $picker->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $picker->move(1),
            $key === Key::ENTER => $this->chooseBackLink(),
            default => $picker->type($key),
        };
    }

    private function chooseBackLink(): void
    {
        $chosen = $this->linkPicker?->selected();
        $link = $chosen === null ? null : ($this->linkChoices[$chosen] ?? null);

        $this->linkPicker = null;

        if ($link !== null) {
            $this->openBackLink($link);
        }
    }

    /**
     * Escape means "back where I was" while a jump is on the stack, which is
     * what following a link makes you want. With nothing to go back to it
     * stays quiet rather than reporting on every stray press.
     */
    private function escape(): bool
    {
        // A jump carries its own filter, so going back comes first.
        if ($this->jumps !== []) {
            return $this->jumpBack();
        }

        if ($this->filters !== null) {
            return $this->clearFilters();
        }

        // A hand-written query replaces the grid with its results. Escape is
        // how you get the table back, since nothing else on screen says where
        // those rows came from.
        if ($this->resultsFromQuery) {
            $this->load();
            $this->status = 'back to '.$this->currentTable();

            return true;
        }

        return true;
    }

    private function jumpBack(): bool
    {
        $jump = array_pop($this->jumps);

        if ($jump === null) {
            $this->status = 'nowhere to go back to';

            return true;
        }

        if (! in_array($jump['table'], $this->tables, true)) {
            return true;
        }

        $this->openLinked($jump['table'], $jump['filters']);

        $this->rowIndex = min($jump['row'], max(0, count($this->rows) - 1));
        $this->status = 'back in '.$jump['table'];

        return true;
    }

    /**
     * Open a table by name.
     *
     * The sidebar filter decides which tables are visible, and every index in
     * the app is an index into that visible list — so a jump to a table the
     * filter is hiding has to clear the filter, or it lands on nothing.
     */
    private function openLinked(string $table, ?Filters $filters): void
    {
        if (! in_array($table, $this->visibleTables(), true)) {
            $this->filter = null;
            $this->filtering = false;
        }

        $at = array_search($table, $this->visibleTables(), true);

        if ($at === false) {
            return;
        }

        $this->tableIndex = $at;
        $this->filters = $filters;
        $this->offset = 0;
        $this->sortColumn = null;
        $this->sortDirection = 'asc';
        $this->pendingDeletes = [];
        $this->pendingEdits = [];
        $this->focus = 'grid';

        $this->load();
    }

    private function openFilters(): bool
    {
        if ($this->headers === []) {
            $this->status = 'open a table first';

            return true;
        }

        $this->filterForm = new FilterForm(
            $this->headers,
            $this->filters,
            $this->headers[$this->columnIndex] ?? null,
        );
        $this->status = null;

        return true;
    }

    /**
     * The letters that drive the filter form rather than typing into it.
     */
    private const FILTER_KEYS = ['h', 'j', 'k', 'l', 'n', 'd', 'o', 'i', ' ', '+', '-'];

    private function handleFilterFormKey(string $key): void
    {
        $form = $this->filterForm;

        if ($form->picker !== null) {
            match (true) {
                $key === Key::ESCAPE => $form->closePicker(),
                $key === Key::ENTER => $form->choose(),
                in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->picker->move(-1),
                in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->picker->move(1),
                default => $form->picker->type($key),
            };

            return;
        }

        if ($form->editor !== null) {
            match (true) {
                $key === Key::ESCAPE => $form->abandon(),
                $key === Key::ENTER, $key === self::SAVE => $form->commit(),
                default => $form->editor->handle($key),
            };

            if ($key === self::SAVE) {
                $this->applyFilters();
            }

            return;
        }

        // On the value, enter applies: the first enter left the input, so the
        // second is the one that runs it.
        if ($form->cell === FilterForm::VALUE) {
            if ($key === Key::ENTER) {
                $this->applyFilters();

                return;
            }

            // Typing goes straight back into the input, with the key you hit
            // — except the keys that move around the form, which would
            // otherwise be impossible to use once escape had stepped out of
            // the value. i or enter starts typing one of those.
            $text = Input::text($key);

            if ($text !== '' && ! in_array($key, self::FILTER_KEYS, true)) {
                $form->startEditing();
                $form->editor?->handle($key);

                return;
            }
        }

        match (true) {
            $key === Key::ESCAPE => $this->clearFilters(),
            $key === self::SAVE => $this->applyFilters(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->moveRow(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->moveRow(1),
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $this->moveOrCycle($form, -1),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l'], true) => $this->moveOrCycle($form, 1),
            $key === Key::TAB => $form->moveCell(1),
            $key === Key::SHIFT_TAB => $form->moveCell(-1),
            $key === '+', $key === 'n' => $form->add(),
            $key === '-', $key === 'd' => $form->remove(),
            $key === 'o' => $form->toggleJoiner(),
            $key === Key::ENTER, $key === 'i' => $form->cell === FilterForm::VALUE
                ? $form->startEditing()
                : $form->openPicker(),
            default => true,
        };
    }

    /**
     * On the column and operator cells the arrows change the value, which is
     * what you want there; tab is how you move between cells.
     */
    private function moveOrCycle(FilterForm $form, int $by): void
    {
        if ($form->cell === FilterForm::VALUE) {
            $form->moveCell($by);

            return;
        }

        $form->cycle($by);
    }

    private function applyFilters(): bool
    {
        $form = $this->filterForm;

        $form?->commit();

        $filters = $form?->toFilters();

        $this->filters = $filters !== null && $filters->usable() ? $filters : null;
        $this->filterForm = null;

        $this->offset = 0;
        $this->pendingDeletes = [];
        $this->pendingEdits = [];

        $this->load();

        return true;
    }

    private function clearFilters(): bool
    {
        $this->filterForm = null;

        if ($this->filters === null) {
            $this->status = null;

            return true;
        }

        $this->filters = null;
        $this->offset = 0;

        $this->load();

        $this->status = 'filter cleared';

        return true;
    }

    private function openQuestion(): bool
    {
        $this->question = new QueryEditor;
        $this->status = null;

        return true;
    }

    private function handleQuestionKey(string $key): void
    {
        if ($key === Key::ESCAPE) {
            $this->question = null;
            $this->status = null;

            return;
        }

        if (in_array($key, self::NEWLINE, true)) {
            $this->question?->handle(Key::ENTER);

            return;
        }

        // Enter sends it; shift+enter above is how you get a second line.
        if ($key === Key::ENTER || $key === self::ASK) {
            $question = trim($this->question?->buffer() ?? '');

            if ($question === '') {
                return;
            }

            $this->askFor($question);

            return;
        }

        $this->question?->handle($key);
    }

    /**
     * Put the answer in the editor rather than running it, with the
     * explanation as comments above it so it reads in place and still runs.
     */
    private function askFor(string $question): bool
    {
        // Paint the waiting state before the call blocks the loop.
        $this->asking = 'asking…';
        $this->render();

        $answer = app(Ask::class)->for($this->connection, $question, $this->currentTable());

        $this->asking = null;

        if (is_string($answer)) {
            $this->status = $answer;

            return true;
        }

        if ($answer['query'] === '') {
            $this->status = $answer['notes'] !== '' ? $answer['notes'] : 'no query for that';

            return true;
        }

        $this->question = null;

        $this->editor->set($this->annotate($answer));
        $this->editor->toStart();

        $this->enterQueryMode();
        $this->status = $this->runKey().' runs it · read it first · esc returns';

        return true;
    }

    /**
     * @param  array{query: string, explanation: string, notes: string}  $answer
     */
    private function annotate(array $answer): string
    {
        $lines = [];

        foreach ([$answer['explanation'], $answer['notes']] as $text) {
            if ($text === '') {
                continue;
            }

            foreach (explode("\n", wordwrap($text, 76)) as $line) {
                $lines[] = '-- '.$line;
            }

            $lines[] = '--';
        }

        return implode("\n", $lines).($lines === [] ? '' : "\n").static::trimmed($answer['query']);
    }

    /**
     * A model often signs off with its own block comment after the statement.
     * The explanation is already above the query, and a comment nobody asked
     * for runs off the side of the editor.
     */
    private static function trimmed(string $query): string
    {
        return rtrim((string) preg_replace('#/\*.*?\*/\s*$#s', '', trim($query)));
    }

    private function openFilter(): bool
    {
        $this->filtering = true;
        $this->filter ??= '';
        $this->tablesHidden = false;
        $this->focus = 'sidebar';
        $this->status = 'filtering tables · ↵ keeps it · esc clears it';

        return true;
    }

    private function handleFilterKey(string $key): void
    {
        if ($key === Key::ESCAPE) {
            $this->filtering = false;
            $this->filter = null;
            $this->status = null;
            $this->reselect();

            return;
        }

        if ($key === Key::ENTER) {
            $this->filtering = false;
            $this->status = $this->filter === '' ? null : 'filtered by "'.$this->filter.'"';

            return;
        }

        if (in_array($key, [Key::BACKSPACE, Key::CTRL_H], true)) {
            $this->filter = mb_substr((string) $this->filter, 0, -1);
            $this->reselect();

            return;
        }

        $text = Input::text($key);

        if ($text !== '') {
            $this->filter .= $text;
            $this->reselect();
        }
    }

    /**
     * Keep the cursor on a table that still exists after the filter changes,
     * and follow it into the grid so the two panes never disagree.
     */
    private function reselect(): void
    {
        $visible = $this->visibleTables();

        if ($visible === []) {
            return;
        }

        $this->tableIndex = min($this->tableIndex, count($visible) - 1);

        $this->offset = 0;
        $this->pendingDeletes = [];
        $this->sortColumn = null;
        $this->load();
    }

    private const PALETTE_SKIPS = ['move_up', 'move_down', 'move_left', 'move_right', 'activate', 'escape', 'command', 'palette'];

    private const PALETTE_COMMANDS = [
        'w' => 'write the pending changes',
        'export' => 'export this table as SQL',
        'tables' => 'focus the table list',
        'rows' => 'focus the rows',
    ];

    private function openPalette(): bool
    {
        $items = [];

        foreach (Keymap::all() as $binding) {
            if (in_array($binding->action, self::PALETTE_SKIPS, true)) {
                continue;
            }

            $items[] = Palette::item(Palette::ACTION, ucfirst($binding->description), $this->keysOf($binding), $binding->action);
        }

        foreach (self::PALETTE_COMMANDS as $command => $description) {
            $items[] = Palette::item(Palette::COMMAND, ucfirst($description), ':'.$command, $command);
        }

        foreach ($this->tables as $table) {
            $items[] = Palette::item(Palette::TABLE, $table, 'table', $table);
        }

        if ($this->connection->driver !== 'sqlite') {
            foreach ($this->runner->databases($this->connection) as $database) {
                if ($database !== $this->connection->activeDatabase()) {
                    $items[] = Palette::item(Palette::DATABASE, $database, 'database', $database);
                }
            }
        }

        foreach (Connection::orderBy('name')->get() as $other) {
            if ($other->id !== $this->connection->id) {
                $items[] = Palette::item(Palette::CONNECTION, $other->name, 'connection', (string) $other->id);
            }
        }

        $this->palette = new Palette($items);
        $this->status = null;

        return true;
    }

    private function keysOf(Binding $binding): string
    {
        return implode(' / ', array_values(array_unique(array_map(Keys::glyph(...), $binding->keys))));
    }

    private function handlePaletteKey(string $key): void
    {
        $palette = $this->palette;

        match (true) {
            $key === Key::ESCAPE => $this->palette = null,
            $key === Key::ENTER => $this->runPaletteItem($palette->selected()),
            in_array($key, [Key::UP, Key::UP_ARROW, Key::CTRL_P], true) => $palette->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, Key::CTRL_N], true) => $palette->move(1),
            default => $palette->type($key),
        };
    }

    private function runPaletteItem(?array $item): void
    {
        $this->palette = null;

        if ($item === null) {
            return;
        }

        if ($this->mode === 'query' && $item['target'] !== 'sql') {
            $this->mode = 'browse';
        }

        match ($item['kind']) {
            Palette::ACTION => $this->runAction($item['target']),
            Palette::COMMAND => $this->runCommand($item['target']),
            Palette::TABLE => $this->goToTable($item['target']),
            Palette::DATABASE => $this->useDatabase($item['target']),
            Palette::CONNECTION => $this->quit('open:'.$item['target']),
            default => null,
        };
    }

    private function goToTable(string $table): void
    {
        $this->filter = null;
        $this->resultsFromQuery = false;

        $at = array_search($table, $this->visibleTables(), true);

        if ($at === false) {
            return;
        }

        $this->selectTable($at);
        $this->focus = 'grid';
    }

    private function openCommandLine(): bool
    {
        $this->command = '';

        return true;
    }

    private function handleCommandKey(string $key): bool
    {
        if ($key === Key::ESCAPE) {
            $this->command = null;

            return true;
        }

        if ($key === Key::ENTER) {
            return (bool) $this->runCommand(trim($this->command ?? ''));
        }

        if (in_array($key, [Key::BACKSPACE, Key::CTRL_H], true)) {
            $this->command = mb_substr($this->command, 0, -1);

            return true;
        }

        $this->command .= Input::text($key);

        return true;
    }

    private function runCommand(string $command): bool
    {
        $this->command = null;

        return match ($command) {
            'q', 'q!', 'quit' => $this->quit(),
            'c', 'connections' => $this->quit('connections'),
            'tables' => $this->focusOn('sidebar'),
            'rows' => $this->focusOn('grid'),
            'r' => $this->runOrReload(),
            'run' => $this->runFromEditor(),
            'reload' => $this->reload(),
            'w', 'write' => $this->writePending(),
            'sql' => $this->openQuery(),
            'export', 'export sql' => $this->export(),
            default => $this->unknownCommand($command),
        };
    }

    private function unknownCommand(string $command): bool
    {
        $this->status = "unknown command: :{$command}";

        return true;
    }

    public function pendingCallout(): string
    {
        return $this->hasPending() ? $this->pendingStatus() : '';
    }

    public function hasPending(): bool
    {
        return $this->pendingDeletes !== [] || $this->pendingEdits !== [] || $this->pendingInserts !== [];
    }

    public function canGoBack(): bool
    {
        return $this->jumps !== [] || $this->filters !== null;
    }

    public function tablesWidth(): int
    {
        return $this->sidebarWidth ?? Layout::sidebarWidth();
    }

    private function toggleTables(): bool
    {
        $this->tablesHidden = ! $this->tablesHidden;

        if ($this->tablesHidden && $this->focus === 'sidebar') {
            $this->focus = 'grid';
        }

        $this->status = $this->tablesHidden
            ? 'table list hidden · '.Keymap::key('toggle_tables').' shows it'
            : null;

        return true;
    }

    private function showTables(): bool
    {
        $this->tablesHidden = false;

        return $this->focusPane('sidebar');
    }

    private function resizeTables(?int $by): bool
    {
        $this->sidebarWidth = $by === null
            ? null
            : max(8, min(80, $this->tablesWidth() + $by));

        $this->status = 'table list width '.$this->tablesWidth();

        return true;
    }

    private function focusPane(string $pane): bool
    {
        $this->mode = 'browse';
        $this->focus = $pane;

        return true;
    }

    private function focusOn(string $pane): bool
    {
        $this->focus = $pane;

        return true;
    }

    private function toggleFocus(int $step = 1): bool
    {
        $panes = Layout::sqlAlways() || $this->mode === 'query'
            ? ['sidebar', 'grid', 'sql']
            : ['sidebar', 'grid'];

        if ($this->tablesHidden) {
            $panes = array_values(array_diff($panes, ['sidebar']));
        }

        $current = $this->mode === 'query' ? 'sql' : $this->focus;
        $at = array_search($current, $panes, true);
        $at = $at === false ? 0 : $at;

        $next = $panes[($at + $step + count($panes)) % count($panes)];

        if ($next === 'sql') {
            return $this->openQuery();
        }

        $this->mode = 'browse';
        $this->focus = $next;

        return true;
    }

    private function moveUp(): bool
    {
        if ($this->focus === 'sidebar') {
            return $this->selectTable(max(0, $this->tableIndex - 1));
        }

        $this->rowIndex = max(0, $this->rowIndex - 1);

        return true;
    }

    private function moveDown(): bool
    {
        if ($this->focus === 'sidebar') {
            return $this->selectTable(min(count($this->visibleTables()) - 1, $this->tableIndex + 1));
        }

        $this->rowIndex = min(max(0, count($this->rows) - 1), $this->rowIndex + 1);

        return true;
    }

    private function selectTable(int $index): bool
    {
        if ($index === $this->tableIndex) {
            return true;
        }

        $this->tableIndex = $index;
        $this->offset = 0;

        // Marks name rows by primary key, so they mean nothing in another
        // table — and writing them there would delete the wrong rows. A row
        // waiting to be added belongs to the table it was added to.
        $this->pendingDeletes = [];
        $this->pendingEdits = [];
        $this->pendingInserts = [];
        $this->filters = null;

        $this->sortColumn = null;
        $this->sortDirection = 'asc';
        $this->load();

        return true;
    }

    private function moveColumn(int $by): bool
    {
        if ($this->headers === []) {
            return true;
        }

        $this->columnIndex = max(0, min(count($this->headers) - 1, $this->columnIndex + $by));

        return true;
    }

    private function activate(): bool
    {
        if ($this->focus === 'sidebar') {
            $this->offset = 0;
            $this->load();
            $this->focus = 'grid';

            return true;
        }

        return $this->startEditing();
    }

    private function page(int $by): bool
    {
        $next = $this->offset + $by;

        if ($next < 0 || ($by > 0 && ! $this->hasMore)) {
            return true;
        }

        $this->offset = $next;
        $this->load();

        return true;
    }

    private function reload(): bool
    {
        // Re-read the tables too: reload should mean the whole picture, so a
        // table created since you opened the connection shows up. Same for the
        // schema tql has been remembering between frames.
        $table = $this->currentTable();

        $this->runner->forgetSchema();

        $this->tables = $this->runner->tables($this->connection);

        $at = $table === null ? false : array_search($table, $this->visibleTables(), true);
        $this->tableIndex = $at === false ? min($this->tableIndex, max(0, count($this->visibleTables()) - 1)) : $at;

        $this->load(keepCursor: true);

        return true;
    }

    /**
     * @param  bool  $keepCursor  Stay on the same column and row. Sorting and
     *                            reloading are about the rows you are already
     *                            looking at, so throwing the cursor back to
     *                            the first cell loses your place.
     */
    private function load(bool $keepCursor = false): void
    {
        $table = $this->currentTable();

        if ($table === null) {
            return;
        }

        $this->sortColumn ??= $this->defaultSort();

        $column = $this->headers[$this->columnIndex] ?? null;
        $row = $this->rowIndex;

        $result = $this->runner->rows(
            $this->connection,
            $table,
            self::PAGE + 1,
            $this->offset,
            $this->sortColumn,
            $this->sortDirection,
            $this->filters,
        );

        if ($result->failed()) {
            $this->status = $result->error;
            $this->headers = [];
            $this->rows = [];
            $this->raw = [];

            return;
        }

        $rows = $result->rows;

        $this->hasMore = count($rows) > self::PAGE;

        if ($this->hasMore) {
            array_pop($rows);
        }

        $this->resultsFromQuery = false;
        $this->queryTable = null;
        $this->lastStatement = $result->statement === null
            ? null
            : preg_replace('/\blimit \d+/i', 'limit '.self::PAGE, $result->statement);
        $this->headers = $result->headers();
        $this->raw = $rows;
        $this->rows = $this->formatter->rows($rows);
        $this->rowIndex = 0;
        $this->columnIndex = 0;
        $this->columnOffset = 0;

        // Rows waiting to be written belong to this table, so they come back
        // with it after a sort, a page or a reload.
        $this->appendPendingRows();

        if ($keepCursor) {
            $index = $column === null ? false : array_search($column, $this->headers, true);

            $this->columnIndex = $index === false
                ? min($this->columnIndex, max(0, count($this->headers) - 1))
                : $index;

            $this->rowIndex = min($row, max(0, count($this->rows) - 1));
        }

        if ($this->mode !== 'query' && $this->lastStatement !== null) {
            $this->editor->set($this->lastStatement);
        }

        $first = count($rows) === 0 ? 0 : $this->offset + 1;
        $last = $this->offset + count($rows);

        $this->status = "rows {$first}-{$last}".($this->hasMore ? '+' : '')." · {$result->durationMs}ms";
    }
}
