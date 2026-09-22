<?php

namespace App\Tui;

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Prompts\Renderers\BrowserRenderer;
use Chewie\Concerns\CreatesAnAltScreen;
use Chewie\Concerns\RegistersRenderers;
use Laravel\Prompts\Key;
use Laravel\Prompts\Prompt;

class Browser extends Prompt
{
    use CreatesAnAltScreen;
    use RegistersRenderers;

    public const PAGE = 100;

    public array $tables = [];

    public int $tableIndex = 0;

    public array $headers = [];

    public array $rows = [];

    public int $rowIndex = 0;

    public int $columnOffset = 0;

    public int $offset = 0;

    public string $focus = 'sidebar';

    public ?string $status = null;

    public ?string $command = null;

    public function __construct(
        public Connection $connection,
        private QueryRunner $runner,
        private RowFormatter $formatter,
    ) {
        $this->registerRenderer(BrowserRenderer::class);

        $this->tables = $this->runner->tables($this->connection);

        $this->createAltScreen();

        if ($this->tables !== []) {
            $this->load();
        }

        $this->on('key', fn (string $key) => $this->onKey($key));
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

    public function onKey(string $key): void
    {
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
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $this->scrollColumns(-1),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l'], true) => $this->scrollColumns(1),
            $key === Key::ENTER => $this->activate(),
            $key === 'n' => $this->page(self::PAGE),
            $key === 'p' => $this->page(-self::PAGE),
            default => true,
        };
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
            $this->tableIndex = max(0, $this->tableIndex - 1);
        } else {
            $this->rowIndex = max(0, $this->rowIndex - 1);
        }

        return true;
    }

    private function moveDown(): bool
    {
        if ($this->focus === 'sidebar') {
            $this->tableIndex = min(count($this->tables) - 1, $this->tableIndex + 1);
        } else {
            $this->rowIndex = min(max(0, count($this->rows) - 1), $this->rowIndex + 1);
        }

        return true;
    }

    private function scrollColumns(int $by): bool
    {
        $this->columnOffset = max(0, min(max(0, count($this->headers) - 1), $this->columnOffset + $by));

        return true;
    }

    private function activate(): bool
    {
        if ($this->focus === 'sidebar') {
            $this->offset = 0;
            $this->load();
            $this->focus = 'grid';
        }

        return true;
    }

    private function page(int $by): bool
    {
        $next = $this->offset + $by;

        if ($next < 0 || ($by > 0 && count($this->rows) < self::PAGE)) {
            return true;
        }

        $this->offset = $next;
        $this->load();

        return true;
    }

    private function load(): void
    {
        $table = $this->currentTable();

        if ($table === null) {
            return;
        }

        $result = $this->runner->rows($this->connection, $table, self::PAGE, $this->offset);

        if ($result->failed()) {
            $this->status = $result->error;
            $this->headers = [];
            $this->rows = [];

            return;
        }

        $this->headers = $result->headers();
        $this->rows = $this->formatter->rows($result->rows);
        $this->rowIndex = 0;
        $this->columnOffset = 0;
        $this->status = "{$result->count()} rows · {$result->durationMs}ms";
    }
}
