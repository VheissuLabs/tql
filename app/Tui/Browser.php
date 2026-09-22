<?php

namespace App\Tui;

use App\Database\OrderBy;
use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RendersSmoothly;
use App\Tui\Islands\EditorIsland;
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

    public const PAGE = 100;

    public array $tables = [];

    public int $tableIndex = 0;

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

    private ?array $drag = null;

    public int $offset = 0;

    public bool $hasMore = false;

    public ?string $lastStatement = null;

    public ?string $queryTable = null;

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

        if ($error = config('dotsql.config_error')) {
            $this->status = $error;
        } elseif ($notice = config('dotsql.config_notice')) {
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

    public function currentTable(): ?string
    {
        return $this->tables[$this->tableIndex] ?? null;
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
            if (in_array($key, [Key::ESCAPE, '?', 'q'], true)) {
                $this->mode = 'browse';
            }

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
            $key === '<' => $this->resize(-4),
            $key === '>' => $this->resize(4),
            $key === '=' => $this->resetWidth(),
            $key === 's' => $this->openQuery(),
            $key === 'i' => $this->startEditing(readOnly: true),
            $key === '?' => $this->toggleHelp(),
            $key === 'e' => $this->startEditing(),
            $key === Key::ENTER => $this->activate(),
            $key === 'n' => $this->page(self::PAGE),
            $key === 'p' => $this->page(-self::PAGE),
            $key === 'r' => $this->reload(),
            $key === 'o' => $this->sortBy($this->headers[$this->columnIndex] ?? null),
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
        $this->mode = $this->mode === 'help' ? 'browse' : 'help';

        return true;
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
        $this->readOnlyReason = $readOnly ? 'opened with i — press e to edit' : $this->whyReadOnly();
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

    public function cellColumn(): string
    {
        return $this->headers[$this->columnIndex] ?? '';
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
            $this->cellEditor = null;
            $this->mode = 'browse';
            $this->status = 'edit cancelled';

            return;
        }

        if (in_array($key, ["\x13", Key::CTRL_D], true)) {
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

        $result = $this->runner->update(
            $this->connection,
            $this->currentTable(),
            $column,
            $key,
            $row[$key] ?? null,
            $value === '' ? null : $value,
        );

        if ($result->failed()) {
            $this->status = 'save failed: '.$result->error;

            return;
        }

        $this->reload();

        $this->status = "saved {$column} ({$result->affected} row".($result->affected === 1 ? '' : 's').')';
    }

    /**
     * Rows already arrive in primary key order, so say so: the header gets its
     * marker and the SQL pane shows the order by that is really running.
     */
    private function defaultSort(): ?string
    {
        return $this->keyColumn();
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
        $this->exit = $exit;
        $this->state = 'submit';

        return false;
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

        if (mb_strlen($key) === 1 && ! ctype_cntrl($key)) {
            $this->command .= $key;
        }

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
            return $this->selectTable(min(count($this->tables) - 1, $this->tableIndex + 1));
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
