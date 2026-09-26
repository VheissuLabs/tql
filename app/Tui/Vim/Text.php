<?php

namespace App\Tui\Vim;

final class Text
{
    public readonly int $length;

    private function __construct(public readonly array $chars)
    {
        $this->length = count($chars);
    }

    public static function of(string $text): self
    {
        return new self(mb_str_split($text));
    }

    public function at(int $offset): string
    {
        return $this->chars[$offset] ?? '';
    }

    public function lineStart(int $offset): int
    {
        $offset = min($offset, $this->length);

        while ($offset > 0 && $this->chars[$offset - 1] !== "\n") {
            $offset--;
        }

        return $offset;
    }

    public function lineEnd(int $offset): int
    {
        while ($offset < $this->length && $this->chars[$offset] !== "\n") {
            $offset++;
        }

        return $offset;
    }

    public function lineOf(int $offset): int
    {
        $line = 0;

        for ($index = 0; $index < min($offset, $this->length); $index++) {
            if ($this->chars[$index] === "\n") {
                $line++;
            }
        }

        return $line;
    }

    public function lineCount(): int
    {
        return $this->lineOf($this->length) + 1;
    }

    public function startOfLine(int $line): int
    {
        $offset = 0;

        for ($current = 0; $current < $line; $current++) {
            $offset = $this->lineEnd($offset) + 1;
        }

        return min($offset, $this->length);
    }

    public function isBlankLine(int $offset): bool
    {
        return $this->lineStart($offset) === $this->lineEnd($offset);
    }

    public function slice(int $start, int $end): string
    {
        return implode('', array_slice($this->chars, $start, max(0, $end - $start)));
    }
}
