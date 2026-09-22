<?php

namespace App\Tui;

class RowFormatter
{
    public function __construct(private int $maxWidth = 500) {}

    public function rows(array $rows): array
    {
        return array_map(fn (array $row) => array_map($this->cell(...), $row), $rows);
    }

    private function cell(mixed $value): string
    {
        $text = match (true) {
            $value === null => 'NULL',
            is_bool($value) => $value ? 'true' : 'false',
            is_scalar($value) => (string) $value,
            default => json_encode($value),
        };

        $text = preg_replace('/\s+/', ' ', $text);

        return mb_strlen($text) > $this->maxWidth
            ? mb_substr($text, 0, $this->maxWidth - 1).'…'
            : $text;
    }
}
