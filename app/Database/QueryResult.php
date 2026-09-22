<?php

namespace App\Database;

class QueryResult
{
    public function __construct(
        public readonly array $rows,
        public readonly int $durationMs,
        public readonly ?string $error = null,
    ) {}

    public function failed(): bool
    {
        return $this->error !== null;
    }

    public function headers(): array
    {
        return $this->rows === [] ? [] : array_keys($this->rows[0]);
    }

    public function count(): int
    {
        return count($this->rows);
    }
}
