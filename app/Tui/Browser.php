<?php

namespace App\Tui;

use App\Database\QueryRunner;
use App\Database\SqlExporter;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RendersSmoothly;
use App\Tui\Islands\SidebarIsland;
use App\Tui\Islands\TableIsland;
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

    private ?array $drag = null;

    public int $offset = 0;

    public bool $hasMore = false;

    public string $mode = 'browse';

    public QueryEditor $editor;

    public bool $resultsFromQuery = false;

    public string $focus = 'sidebar';

    public ?string $status = null;

    public ?string $command = null;

    public ?string $editing = null;

    public bool $debugMouse = false;

    public ?array $lastMouse = null;

    public int $inspectOffset = 0;

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
        return null;
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

        if ($this->mode === 'help') {
            if (in_array($key, [Key::ESCAPE, '?', 'q'], true)) {
                $this->mode = 'browse';
            }

            return;
        }

        if ($this->mode === 'inspect') {
            match (true) {
                in_array($key, [Key::ESCAPE, 'i', 'q'], true) => $this->mode = 'browse',
                in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->inspectOffset++,
                in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->inspectOffset = max(0, $this->inspectOffset - 1),
                $key === 'n' => $this->inspectOffset += 10,
                $key === 'p' => $this->inspectOffset = max(0, $this->inspectOffset - 10),
                default => null,
            };

            return;
        }

        if ($this->mode === 'query') {
            $this->handleQueryKey($key);

            return;
        }

        if ($this->editing !== null) {
            $this->handleEditKey($key);

            return;
        }

        if ($this->command !== null) {
            $this->handleCommandKey($key);

            return;
        }

        match (true) {
            $key === ':' => $this->openCommandLine(),
            $key === 'q', $key === Key::ESCAPE => $this->quit(),
            $key === Key::TAB => $this->toggleFocus(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->moveUp(),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->moveDown(),
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $this->moveColumn(-1),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l'], true) => $this->moveColumn(1),
            $key === '<' => $this->resize(-4),
            $key === '>' => $this->resize(4),
            $key === '=' => $this->resetWidth(),
            $key === 's' => $this->openQuery(),
            $key === 'i' => $this->toggleInspect(),
            $key === '?' => $this->toggleHelp(),
            $key === 'e' => $this->startEditing(),
            $key === Key::ENTER => $this->activate(),
            $key === 'n' => $this->page(self::PAGE),
            $key === 'p' => $this->page(-self::PAGE),
            $key === 'r' => $this->reload(),
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

    private function toggleMouseDebug(): bool
    {
        $this->debugMouse = ! $this->debugMouse;
        $this->status = $this->debugMouse ? 'mouse debug on — click anything' : 'mouse debug off';

        return true;
    }

    private function toggleHelp(): bool
    {
        $this->mode = $this->mode === 'help' ? 'browse' : 'help';

        return true;
    }

    private function toggleInspect(): bool
    {
        if ($this->mode === 'inspect') {
            $this->mode = 'browse';

            return true;
        }

        if ($this->focus !== 'grid' || $this->raw === []) {
            return true;
        }

        $this->mode = 'inspect';
        $this->inspectOffset = 0;

        return true;
    }

    public function cellColumn(): string
    {
        return $this->headers[$this->columnIndex] ?? '';
    }

    public function cellIsJson(): bool
    {
        return Json::looksLikeJson($this->cellText());
    }

    public function cellText(): string
    {
        $row = $this->raw[$this->rowIndex] ?? null;
        $column = $this->headers[$this->columnIndex] ?? null;

        if ($row === null || $column === null) {
            return '';
        }

        $value = $row[$column] ?? null;

        return $value === null ? 'NULL' : (string) $value;
    }

    private function openQuery(): bool
    {
        $this->mode = 'query';
        $this->status = 'ctrl+r runs the query · esc returns';

        return true;
    }

    private function handleQueryKey(string $key): void
    {
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

        $this->status = "{$result->count()} rows · {$result->durationMs}ms";
    }

    private function startEditing(): bool
    {
        if ($this->focus !== 'grid') {
            return true;
        }

        if ($this->connection->read_only) {
            $this->status = 'this connection is marked read-only';

            return true;
        }

        if ($this->raw === []) {
            return true;
        }

        if ($this->resultsFromQuery) {
            $this->status = 'query results are read-only — open a table to edit';

            return true;
        }

        if ($this->keyColumn() === null) {
            $this->status = 'cannot edit: '.$this->currentTable().' has no single-column primary key';

            return true;
        }

        $value = $this->cellValue();

        $this->editing = $value === null ? '' : (string) $value;

        return true;
    }

    private function handleEditKey(string $key): void
    {
        if ($key === Key::ESCAPE) {
            $this->editing = null;
            $this->status = 'edit cancelled';

            return;
        }

        if ($key === Key::ENTER) {
            $this->commitEdit();

            return;
        }

        if (in_array($key, [Key::BACKSPACE, Key::CTRL_H], true)) {
            $this->editing = mb_substr($this->editing, 0, -1);

            return;
        }

        if (mb_strlen($key) === 1 && ! ctype_cntrl($key)) {
            $this->editing .= $key;
        }
    }

    private function commitEdit(): void
    {
        $value = $this->editing;
        $this->editing = null;

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
        $debug = $this->debugMouse && $event['pressed'] && $event['button'] === Mouse::LEFT;

        if ($debug) {
            $this->lastMouse = $event + [
                'sidebar' => $this->sidebar?->containsContent($event['column'], $event['row']) ?? false,
                'table' => $this->table?->containsContent($event['column'], $event['row']) ?? false,
                'sidebarY' => $this->sidebar?->y,
                'tableY' => $this->table?->y,
                'localRow' => $this->sidebar?->localRow($event['row']),
                'before' => $this->currentTable(),
            ];
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

        if ($debug) {
            $this->lastMouse['selected'] = $this->currentTable();
            $this->lastMouse['rowIndex'] = $this->rowIndex;
        }
    }

    private function press(int $column, int $row): bool
    {
        if ($this->table !== null && $row === $this->table->y + 1) {
            $handle = $this->handleNear($column);

            if ($handle !== null) {
                $name = $this->headers[$handle] ?? null;

                if ($name !== null) {
                    $this->drag = ['column' => $name, 'index' => $handle];

                    $this->columnIndex = $handle;

                    return true;
                }
            }
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

    private function quit(): bool
    {
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
            'tables' => $this->focusOn('sidebar'),
            'rows' => $this->focusOn('grid'),
            'r', 'reload' => $this->reload(),
            'sql' => $this->openQuery(),
            'export', 'export sql' => $this->export(),
            'mouse' => $this->toggleMouseDebug(),
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

    private function toggleFocus(): bool
    {
        $this->focus = $this->focus === 'sidebar' ? 'grid' : 'sidebar';

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
        $row = $this->rowIndex;
        $column = $this->columnIndex;

        $this->load();

        $this->rowIndex = min($row, max(0, count($this->rows) - 1));
        $this->columnIndex = min($column, max(0, count($this->headers) - 1));

        return true;
    }

    private function load(): void
    {
        $table = $this->currentTable();

        if ($table === null) {
            return;
        }

        $result = $this->runner->rows($this->connection, $table, self::PAGE + 1, $this->offset);

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
        $this->headers = $result->headers();
        $this->raw = $rows;
        $this->rows = $this->formatter->rows($rows);
        $this->rowIndex = 0;
        $this->columnIndex = 0;
        $this->columnOffset = 0;

        $first = count($rows) === 0 ? 0 : $this->offset + 1;
        $last = $this->offset + count($rows);

        $this->status = "rows {$first}-{$last}".($this->hasMore ? '+' : '')." · {$result->durationMs}ms";
    }
}
