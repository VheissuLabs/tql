<?php

namespace App\Tui;

use App\Ai\Ask;
use App\Database\Filter;
use App\Database\Filters;
use App\Database\OrderBy;
use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RendersSmoothly;
use App\Tui\Islands\EditorIsland;
use App\Tui\Islands\HelpIsland;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\TableIsland;
use App\Tui\Islands\ValueEditorIsland;
use Chewie\Concerns\CreatesAnAltScreen;
use Chewie\Concerns\RegistersRenderers;
use Laravel\Prompts\Key;
use Laravel\Prompts\Prompt;

class Browser extends Prompt
{
    use CreatesAnAltScreen;
    use HandlesMouse;
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

    public ?TableIsland $table = null;

    public ?ValueEditorIsland $valueIsland = null;

    public ?EditorIsland $editorIsland = null;

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

    public ?string $sortColumn = null;

    public string $sortDirection = 'asc';

    public string $mode = 'browse';

    public QueryEditor $editor;

    public bool $resultsFromQuery = false;

    public string $focus = 'sidebar';

    public ?string $status = null;

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

        $this->editor = new QueryEditor;

        $this->tables = $this->runner->tables($this->connection);

        $this->createAltScreen();

        if ($this->tables !== []) {
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
        ]);
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
        if ($event = Mouse::parse($key)) {
            $this->onMouse($event);

            return;
        }

        // ctrl+l, the usual way out of a terminal that has been left dirty by
        // a resize, a notification, or anything else writing over the frame.
        if ($key === "\x0c") {
            $this->repaint();
            $this->status = 'redrawn';

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

        match (true) {
            $key === ':' => $this->openCommandLine(),
            $key === 'q' => $this->quit(),
            $key === Key::TAB => $this->toggleFocus(),
            $key === Key::SHIFT_TAB => $this->toggleFocus(-1),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->moveUp(),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->moveDown(),
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $this->moveColumn(-1),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l'], true) => $this->moveColumn(1),
            // , and . are the unshifted < and >, so the pair is the same two
            // keys without the reach. Both spellings work.
            $key === ',', $key === '<' => $this->resize(-4),
            $key === '.', $key === '>' => $this->resize(4),
            $key === '=' => $this->resetWidth(),
            $key === 's' => $this->openQuery(),
            $key === 'i' => $this->inspectRow(),
            $key === 'I' => $this->startEditing(readOnly: true),
            $key === '?' => $this->toggleHelp(),
            $key === 'e' => $this->startEditing(),
            $key === Key::ENTER => $this->activate(),
            $key === 'n' => $this->page(self::PAGE),
            $key === 'p' => $this->page(-self::PAGE),
            $key === 'r' => $this->reload(),
            $key === 'o' => $this->sortBy($this->headers[$this->columnIndex] ?? null),
            $key === '/' => $this->openFilter(),
            $key === 'a' => $this->openQuestion(),
            $key === 'f' => $this->openFilters(),
            $key === 't' => $this->toggleStructure(),
            $key === 'L' => $this->followLink(),
            $key === self::BACK => $this->jumpBack(),
            $key === Key::ESCAPE => $this->escape(),
            $key === 'd' => $this->markDelete(),
            $key === 'u' => $this->unmarkAll(),
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
        $this->mode = 'query';

        if ($this->editor->isEmpty() && $this->lastStatement !== null) {
            $this->editor->set($this->lastStatement);
        }

        $this->status = 'ctrl+r runs it · esc returns · edit it and run it again';

        return true;
    }

    private function handleQueryKey(string $key): void
    {
        if ($key === Key::TAB || $key === Key::SHIFT_TAB) {
            $this->toggleFocus($key === Key::SHIFT_TAB ? -1 : 1);

            return;
        }

        if ($key === Key::ESCAPE) {
            $this->mode = 'browse';
            $this->status = null;

            return;
        }

        if ($key === QueryEditor::RUN) {
            $this->runQueryBuffer();

            return;
        }

        $this->editor->handle($key);
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

    private function runQueryBuffer(): void
    {
        if ($this->editor->isEmpty()) {
            $this->status = 'nothing to run';

            return;
        }

        $result = $this->runner->run($this->connection, $this->editor->buffer(), 'tui');

        if ($result->failed()) {
            $this->status = $result->error;

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
        [$this->sortColumn, $this->sortDirection] = OrderBy::of($this->editor->buffer()) ?? [null, 'asc'];

        $this->followQueryTable($this->editor->buffer());

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

    public int $documentLine = 0;

    public ?int $documentAnchor = null;

    public int $structureOffset = 0;

    /** Set by the renderer, so scrolling knows where the list ends. */
    public int $structureHidden = 0;

    /** @var array<int, array{table: string, filters: Filters|null, row: int}> */
    private array $jumps = [];

    /** The list open for choosing which way to follow a row. */
    public ?Picker $linkPicker = null;

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
        $this->documentLine = 0;
        $this->documentAnchor = null;

        $this->document = new RowDocument(
            $this->readable($row),
            $this->columnTypes(),
            $this->relatedRecords($row),
        );

        $this->mode = 'inspect';

        return true;
    }

    /**
     * @return array<string, string>
     */
    private function columnTypes(): array
    {
        $table = $this->currentTable();

        if ($table === null) {
            return [];
        }

        $types = [];

        foreach ($this->columnsOf($table) as $column) {
            $types[(string) ($column['name'] ?? '')] = (string) ($column['type_name'] ?? $column['type'] ?? '');
        }

        return $types;
    }

    /**
     * @param  array<string, mixed>  $row
     * @return array<string, array{rows: array<int, array<string, mixed>>, total: ?int}>
     */
    private function relatedRecords(array $row): array
    {
        $limit = Layout::inspectRelated();

        if ($limit < 1) {
            return [];
        }

        $related = [];

        foreach ($this->links() as $column => $link) {
            $value = $row[$column] ?? null;

            if ($value === null) {
                continue;
            }

            $parent = $this->runner->related($this->connection, $link['table'], $link['column'], $value, 1);

            if ($parent !== []) {
                $related[$link['table']] = [
                    'rows' => [$this->readable($parent[0])],
                    'total' => 1,
                    'hide' => $this->idsToHide($link['table'], $link['column']),
                ];
            }
        }

        foreach ($this->backLinks() as $link) {
            $value = $row[$link['references']] ?? null;

            if ($value === null) {
                continue;
            }

            $rows = $this->runner->related($this->connection, $link['table'], $link['column'], $value, $limit);

            if ($rows === []) {
                continue;
            }

            $more = count($rows) > $limit;

            $related[$link['table']] = [
                'rows' => array_map(fn (array $r) => $this->readable($r), array_slice($rows, 0, $limit)),
                'total' => $more ? $this->countRelated($link, $value) : count($rows),
                'hide' => $this->idsToHide($link['table'], $link['column']),
            ];
        }

        return $related;
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

    /**
     * @param  array{table: string, column: string, references: string}  $link
     */
    private function countRelated(array $link, mixed $value): ?int
    {
        $grammar = $this->runner->grammarFor($this->connection);

        $result = $this->runner->run(
            $this->connection,
            'select count(*) as total from '.$grammar->wrapTable($link['table']).
                ' where '.$grammar->wrap($link['column']).' = ?',
            'tui',
            [$value],
        );

        return $result->failed() ? null : (int) ($result->rows[0]['total'] ?? 0);
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

            $this->document = null;
            $this->mode = 'browse';
            $this->status = null;

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
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->documentLine = min($lines - 1, $this->documentLine + 1),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->documentLine = max(0, $this->documentLine - 1),
            $key === 'g' => $this->documentLine = 0,
            $key === 'G' => $this->documentLine = $lines - 1,
            $key === Key::ENTER, $key === ' ' => $document->toggle($this->documentLine),
            $key === 'V' => $this->documentAnchor = $this->documentAnchor === null ? $this->documentLine : null,
            $key === 'y' => $this->yankDocument(),
            $key === 'i' => $this->foldAll($document),
            default => null,
        };

        $this->documentLine = min($this->documentLine, max(0, count($document->lines()) - 1));
    }

    private function foldAll(RowDocument $document): void
    {
        foreach ([RowDocument::RECORD, RowDocument::RELATED] as $section) {
            if (! $document->isFolded($section)) {
                $document->toggle($this->lineOf($document, $section));
            }
        }

        $this->documentLine = 0;
    }

    private function lineOf(RowDocument $document, string $fold): int
    {
        foreach ($document->lines() as $index => $line) {
            if (($line['fold'] ?? null) === $fold) {
                return $index;
            }
        }

        return 0;
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
            $this->keyColumn() === null => $this->currentTable().' has no single-column primary key',
            default => null,
        };
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

        if (in_array($key, [self::SAVE, Key::CTRL_D], true)) {
            $this->commitEdit();

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

        $this->cellEditor = null;
        $this->mode = 'browse';

        $key = $this->keyColumn();
        $column = $this->headers[$this->columnIndex] ?? null;
        $row = $this->raw[$this->rowIndex] ?? null;

        if ($key === null || $column === null || $row === null) {
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
        if ($this->pendingDeletes === [] && $this->pendingEdits === []) {
            return true;
        }

        $this->pendingDeletes = [];
        $this->pendingEdits = [];
        $this->status = 'pending changes dropped';

        return true;
    }

    private function pendingStatus(): string
    {
        $deletes = count($this->pendingDeletes);
        $edits = count($this->pendingEdits);

        if ($deletes === 0 && $edits === 0) {
            return 'nothing pending';
        }

        $parts = [];

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
        if ($this->pendingDeletes === [] && $this->pendingEdits === []) {
            $this->status = 'nothing to write';

            return true;
        }

        $table = $this->currentTable();
        $key = $this->keyColumn();

        if ($table === null || $key === null) {
            return true;
        }

        $edits = count($this->pendingEdits);
        $deletes = count($this->pendingDeletes);

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
                    $this->status = 'write failed: '.$result->error;

                    return true;
                }
            }
        }

        if ($deletes > 0) {
            $result = $this->runner->delete($this->connection, $table, $key, $this->pendingDeletes);

            if ($result->failed()) {
                $this->status = $result->error;

                return true;
            }
        }

        $this->pendingDeletes = [];
        $this->pendingEdits = [];

        $this->load(keepCursor: true);

        $written = [];

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

        $this->mode = 'query';
        $this->status = 'ctrl+r runs it · esc returns · edit it and run it again';

        if (! $entering || $localRow >= 0) {
            $this->editor->toLineColumn(
                $this->editorIsland->firstLine + max(0, $localRow),
                max(0, $localColumn),
            );
        }

        return true;
    }

    private function clickSidebar(int $localRow): bool
    {
        $target = $this->sidebar?->selectedIndexFor($localRow);

        if ($target === null) {
            return true;
        }

        $this->focus = 'sidebar';
        $this->selectTable($target);
        $this->focus = 'grid';

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
        if (($this->pendingDeletes !== [] || $this->pendingEdits !== []) && $exit === 'quit') {
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
            in_array($key, [Key::UP, Key::UP_ARROW], true) => $picker->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW], true) => $picker->move(1),
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

    private function handleFilterFormKey(string $key): void
    {
        $form = $this->filterForm;

        if ($form->picker !== null) {
            match (true) {
                $key === Key::ESCAPE => $form->closePicker(),
                $key === Key::ENTER => $form->choose(),
                in_array($key, [Key::UP, Key::UP_ARROW], true) => $form->picker->move(-1),
                in_array($key, [Key::DOWN, Key::DOWN_ARROW], true) => $form->picker->move(1),
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
            $key === Key::ENTER => $form->cell === FilterForm::VALUE
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

        $this->mode = 'query';
        $this->status = 'ctrl+r runs it · read it first · esc returns';

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

        return implode("\n", $lines).($lines === [] ? '' : "\n").$answer['query'];
    }

    private function openFilter(): bool
    {
        $this->filtering = true;
        $this->filter ??= '';
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
            'r', 'reload' => $this->reload(),
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
        // table — and writing them there would delete the wrong rows.
        $this->pendingDeletes = [];
        $this->pendingEdits = [];
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
        // table created since you opened the connection shows up.
        $table = $this->currentTable();

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
