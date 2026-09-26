<?php

namespace App\Tui\Vim;

final class Range
{
    public function __construct(
        public readonly int $start,
        public readonly int $end,
        public readonly bool $linewise = false,
    ) {}

    public static function between(int $from, int $to, bool $linewise = false): self
    {
        return new self(min($from, $to), max($from, $to), $linewise);
    }
}
