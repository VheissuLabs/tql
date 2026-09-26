<?php

namespace App\Tui;

use Laravel\Prompts\Key;

class QueryEditor
{
    public const RUN = "\x12";

    private string $buffer = '';

    private int $cursor = 0;

    private array $undone = [];

    private array $redone = [];

    private ?array $pending = null;

    /**
     * A single-line editor flattens pasted newlines instead of smuggling one
     * into a value that has to stay on one line, like a host or a name.
     */
    public function __construct(private bool $multiline = true) {}

    public function buffer(): string
    {
        return $this->buffer;
    }

    public function cursor(): int
    {
        return $this->cursor;
    }

    public function isEmpty(): bool
    {
        return trim($this->buffer) === '';
    }

    public function set(string $buffer): void
    {
        $this->buffer = $buffer;
        $this->cursor = mb_strlen($buffer);
    }

    public function moveTo(int $offset): void
    {
        $this->cursor = max(0, min($offset, mb_strlen($this->buffer)));
    }

    public function replace(int $start, int $end, string $text): void
    {
        $this->buffer = mb_substr($this->buffer, 0, $start).$text.mb_substr($this->buffer, $end);
        $this->cursor = $start + mb_strlen($text);
    }

    public function checkpoint(): void
    {
        $this->pending ??= [$this->buffer, $this->cursor];
    }

    public function commit(): void
    {
        if ($this->pending !== null && $this->pending[0] !== $this->buffer) {
            $this->undone[] = $this->pending;
            $this->redone = [];
        }

        $this->pending = null;
    }

    public function undo(): bool
    {
        $this->commit();

        return $this->step($this->undone, $this->redone);
    }

    public function redo(): bool
    {
        $this->commit();

        return $this->step($this->redone, $this->undone);
    }

    private function step(array &$from, array &$to): bool
    {
        while ($from !== [] && end($from)[0] === $this->buffer) {
            array_pop($from);
        }

        if ($from === []) {
            return false;
        }

        $to[] = [$this->buffer, $this->cursor];
        [$this->buffer, $this->cursor] = array_pop($from);

        return true;
    }

    public function toStart(): void
    {
        $this->cursor = 0;
    }

    public function toLine(int $line): void
    {
        $lines = $this->lines();
        $line = max(0, min($line, count($lines) - 1));

        $offset = 0;

        for ($i = 0; $i < $line; $i++) {
            $offset += mb_strlen($lines[$i]) + 1;
        }

        $this->cursor = $offset;
    }

    public function toLineColumn(int $line, int $column): void
    {
        $this->toLine($line);

        $lines = $this->lines();
        $line = max(0, min($line, count($lines) - 1));

        $this->cursor += max(0, min($column, mb_strlen($lines[$line])));
    }

    public function toEnd(): void
    {
        $this->cursor = mb_strlen($this->buffer);
    }

    public function lines(): array
    {
        return explode("\n", $this->buffer);
    }

    public function cursorLine(): int
    {
        return mb_substr_count(mb_substr($this->buffer, 0, $this->cursor), "\n");
    }

    public function cursorColumn(): int
    {
        $before = mb_substr($this->buffer, 0, $this->cursor);
        $break = mb_strrpos($before, "\n");

        return $break === false ? mb_strlen($before) : mb_strlen($before) - $break - 1;
    }

    public function handle(string $key): void
    {
        match (true) {
            $key === Key::ENTER => $this->insert("\n"),
            in_array($key, [Key::BACKSPACE, Key::CTRL_H], true) => $this->backspace(),
            $key === Key::DELETE => $this->delete(),
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, Key::CTRL_B], true) => $this->move(-1),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, Key::CTRL_F], true) => $this->move(1),
            in_array($key, [Key::UP, Key::UP_ARROW], true) => $this->moveLine(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW], true) => $this->moveLine(1),
            $this->is($key, Key::HOME, Key::CTRL_A) => $this->toLineStart(),
            $this->is($key, Key::END, Key::CTRL_E) => $this->toLineEnd(),
            $key === Key::TAB => $this->insert('  '),
            $key === Key::SHIFT_TAB => $this->outdent(),
            default => $this->type($key),
        };
    }

    private function is(string $key, mixed ...$candidates): bool
    {
        foreach ($candidates as $candidate) {
            if (is_array($candidate) ? in_array($key, $candidate, true) : $key === $candidate) {
                return true;
            }
        }

        return false;
    }

    private function type(string $key): void
    {
        $text = Input::text($key, newlines: $this->multiline);

        if ($text !== '') {
            $this->insert($text);
        }
    }

    private function insert(string $text): void
    {
        $this->buffer = mb_substr($this->buffer, 0, $this->cursor).$text.mb_substr($this->buffer, $this->cursor);
        $this->cursor += mb_strlen($text);
    }

    private function backspace(): void
    {
        if ($this->cursor === 0) {
            return;
        }

        $this->buffer = mb_substr($this->buffer, 0, $this->cursor - 1).mb_substr($this->buffer, $this->cursor);
        $this->cursor--;
    }

    private function delete(): void
    {
        $this->buffer = mb_substr($this->buffer, 0, $this->cursor).mb_substr($this->buffer, $this->cursor + 1);
    }

    private function move(int $by): void
    {
        $this->cursor = max(0, min(mb_strlen($this->buffer), $this->cursor + $by));
    }

    private function moveLine(int $by): void
    {
        $lines = $this->lines();
        $line = $this->cursorLine() + $by;

        if ($line < 0 || $line >= count($lines)) {
            return;
        }

        $column = min($this->cursorColumn(), mb_strlen($lines[$line]));

        $offset = 0;

        for ($i = 0; $i < $line; $i++) {
            $offset += mb_strlen($lines[$i]) + 1;
        }

        $this->cursor = $offset + $column;
    }

    public function toLineStart(): void
    {
        $this->cursor -= $this->cursorColumn();
    }

    public function toLineEnd(): void
    {
        $lines = $this->lines();
        $line = $this->cursorLine();

        $this->cursor += mb_strlen($lines[$line]) - $this->cursorColumn();
    }

    private function outdent(): void
    {
        $line = $this->lines()[$this->cursorLine()];
        $removed = mb_strlen($line) - mb_strlen(preg_replace('/^ {1,2}/', '', $line));

        if ($removed === 0) {
            return;
        }

        $start = $this->cursor - $this->cursorColumn();
        $column = $this->cursorColumn();

        $this->buffer = mb_substr($this->buffer, 0, $start).mb_substr($this->buffer, $start + $removed);
        $this->cursor = $start + max(0, $column - $removed);
    }

    public function openLine(bool $above = false): void
    {
        if ($above) {
            $this->toLineStart();
            $this->insert("\n");
            $this->move(-1);

            return;
        }

        $this->toLineEnd();
        $this->insert("\n");
    }
}
