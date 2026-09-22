<?php

namespace App\Tui;

/**
 * The row inspector's contents: the record itself with its column types, and
 * the records related to it, as sections you can fold.
 *
 * Lines are built from the data each time, so folding is a property of the
 * document rather than something done to a block of text.
 */
class RowDocument
{
    public const RECORD = 'record';

    public const RELATED = 'related';

    /** @var array<string, bool> */
    private array $folded = [];

    /**
     * @param  array<string, mixed>  $row
     * @param  array<string, string>  $types  column => type name
     * @param  array<string, array{rows: array<int, array<string, mixed>>, total: ?int}>  $related
     */
    public function __construct(
        private array $row,
        private array $types = [],
        private array $related = [],
    ) {}

    /**
     * @return array<int, array{text: string, fold: ?string, depth: int, column: ?string}>
     */
    public function lines(): array
    {
        $lines = [];

        $lines[] = $this->heading(self::RECORD, 'record', count($this->row));

        if (! $this->isFolded(self::RECORD)) {
            foreach ($this->row as $column => $value) {
                $lines[] = [
                    'text' => '    '.$this->field((string) $column, $value, 1),
                    'fold' => null,
                    'depth' => 1,
                    'column' => (string) $column,
                ];
            }
        }

        if ($this->related === []) {
            return $lines;
        }

        $lines[] = ['text' => '', 'fold' => null, 'depth' => 0, 'column' => null];
        $lines[] = $this->heading(self::RELATED, 'related', count($this->related));

        if ($this->isFolded(self::RELATED)) {
            return $lines;
        }

        foreach ($this->related as $table => $relation) {
            $key = self::RELATED.'.'.$table;
            $shown = count($relation['rows']);
            $total = $relation['total'];

            $lines[] = [
                'text' => '  '.$this->marker($key).' '.$table.'  '
                    .($total !== null && $total > $shown ? "({$shown} of {$total})" : "({$shown})"),
                'fold' => $key,
                'depth' => 1,
                'column' => null,
            ];

            if ($this->isFolded($key)) {
                continue;
            }

            foreach ($relation['rows'] as $index => $related) {
                foreach ($related as $column => $value) {
                    $lines[] = [
                        'text' => '      '.$this->field((string) $column, $value, 2),
                        'fold' => null,
                        'depth' => 2,
                        'column' => null,
                    ];
                }

                if ($index !== array_key_last($relation['rows'])) {
                    $lines[] = ['text' => '', 'fold' => null, 'depth' => 2, 'column' => null];
                }
            }
        }

        return $lines;
    }

    /**
     * @return array{text: string, fold: ?string, depth: int, column: ?string}
     */
    private function heading(string $key, string $label, int $count): array
    {
        return [
            'text' => $this->marker($key).' '.$label.'  ('.$count.')',
            'fold' => $key,
            'depth' => 0,
            'column' => null,
        ];
    }

    private function marker(string $key): string
    {
        return $this->isFolded($key) ? '▸' : '▾';
    }

    /**
     * Column, value, then type. The value is what you came to read, so it sits
     * next to the name; the type trails it as an annotation.
     */
    private function field(string $column, mixed $value, int $depth): string
    {
        $type = $depth === 1 ? ($this->types[$column] ?? '') : '';
        $value = $this->value($value);

        if ($type === '') {
            return str_pad($column, 22).$value;
        }

        return str_pad($column, 22).str_pad($value, 48).$type;
    }

    private function value(mixed $value): string
    {
        return match (true) {
            $value === null => 'NULL',
            is_bool($value) => $value ? 'true' : 'false',
            is_scalar($value) => (string) $value,
            default => (string) json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
        };
    }

    public function toggle(int $line): void
    {
        $fold = $this->lines()[$line]['fold'] ?? null;

        if ($fold === null) {
            return;
        }

        $this->folded[$fold] = ! $this->isFolded($fold);
    }

    public function foldable(int $line): bool
    {
        return ($this->lines()[$line]['fold'] ?? null) !== null;
    }

    public function isFolded(string $key): bool
    {
        return $this->folded[$key] ?? false;
    }

    /**
     * The column on a line, so pressing e from the inspector edits the right
     * one rather than whichever the grid cursor was on.
     */
    public function columnAt(int $line): ?string
    {
        return $this->lines()[$line]['column'] ?? null;
    }

    public function text(): string
    {
        return implode("\n", array_column($this->lines(), 'text'));
    }
}
