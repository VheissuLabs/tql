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

    private ?array $built = null;

    private ?array $fields = null;

    private array $bodies = [];

    /**
     * @param  array<string, mixed>  $row
     * @param  array<string, string>  $types  column => type name
     * @param  array<string, array{rows: array<int, array<string, mixed>>, total: ?int, hide: array<int, string>, kind?: string}>  $related
     */
    public function __construct(
        private array $row,
        private array $types = [],
        private array $related = [],
    ) {
        $this->folded = collect(array_keys($related))
            ->mapWithKeys(fn ($table) => [self::RELATED.'.'.$table => true])
            ->all();
    }

    /**
     * Every line, in order, tagged with the section it belongs to so the two
     * boxes can be drawn from one list and share one cursor.
     *
     * @return array<int, array{text: string, fold: ?string, section: string, heading: bool, column: ?string}>
     */
    public function lines(): array
    {
        return $this->built ??= $this->build();
    }

    private function build(): array
    {
        $lines = [[
            'text' => 'RECORD  ('.count($this->row).')',
            'fold' => self::RECORD,
            'section' => self::RECORD,
            'heading' => true,
            'column' => null,
        ]];

        if (! $this->isFolded(self::RECORD)) {
            array_push($lines, ...$this->fields ??= array_map(fn ($column, $value) => [
                'text' => '  '.$this->field((string) $column, $value, 1),
                'fold' => null,
                'section' => self::RECORD,
                'heading' => false,
                'column' => (string) $column,
            ], array_keys($this->row), $this->row));
        }

        if ($this->related === []) {
            return $lines;
        }

        $lines[] = [
            'text' => 'RELATED  ('.count($this->related).')',
            'fold' => self::RELATED,
            'section' => self::RELATED,
            'heading' => true,
            'column' => null,
        ];

        if ($this->isFolded(self::RELATED)) {
            return $lines;
        }

        foreach ($this->related as $table => $relation) {
            $key = self::RELATED.'.'.$table;
            $shown = $relation['shown'] ?? count($relation['rows'] ?? []);
            $total = $relation['total'];
            $kind = $relation['kind'] ?? 'has many';
            $one = ! str_starts_with($kind, 'has many');

            $count = $one
                ? ''
                : '  '.($total !== null && $total > $shown ? "({$shown} of {$total})" : "({$shown})");

            $lines[] = [
                'text' => '  '.$this->marker($key).' '.$table.'  ·  '.$kind.$count,
                'fold' => $key,
                'section' => self::RELATED,
                'heading' => false,
                'column' => null,
            ];

            if ($this->isFolded($key)) {
                continue;
            }

            array_push($lines, ...$this->body((string) $table, $relation, $one));
        }

        return $lines;
    }

    private function body(string $table, array $relation, bool $one): array
    {
        if (($relation['rows'] ?? null) === null) {
            return [$this->bodyLine('loading…')];
        }

        return $this->bodies[$table] ??= $this->layOut($table, $relation, $one);
    }

    private function layOut(string $table, array $relation, bool $one): array
    {
        if ($one && count($relation['rows']) === 1) {
            return array_map(
                fn (string $line) => $this->bodyLine($line, $table, 0),
                $this->record($relation['rows'][0], $relation['hide'] ?? []),
            );
        }

        $collection = $this->collection($relation['rows'], $relation['hide'] ?? []);

        return array_map(
            fn (string $line, int $position) => $this->bodyLine(
                $line,
                $table,
                $position === 0
                    ? null
                    : $position - 1,
            ),
            $collection,
            array_keys($collection),
        );
    }

    private function bodyLine(string $text, ?string $table = null, ?int $row = null): array
    {
        return [
            'text' => '      '.$text,
            'fold' => null,
            'section' => self::RELATED,
            'heading' => false,
            'column' => null,
            'table' => $table,
            'row' => $row,
        ];
    }

    /**
     * The lines of one section, with their index in the full list so the
     * cursor and selection still line up.
     *
     * @return array<int, array{text: string, fold: ?string, section: string, heading: bool, column: ?string}>
     */
    public function section(string $section): array
    {
        $found = [];

        foreach ($this->lines() as $index => $line) {
            if ($line['section'] === $section && ! $line['heading']) {
                $found[$index] = $line;
            }
        }

        return $found;
    }

    public function headingAt(string $section): ?int
    {
        foreach ($this->lines() as $index => $line) {
            if ($line['section'] === $section && $line['heading']) {
                return $index;
            }
        }

        return null;
    }

    public function hasRelated(): bool
    {
        return $this->related !== [];
    }

    public function waiting(): array
    {
        if ($this->isFolded(self::RELATED)) {
            return [];
        }

        return collect($this->related)
            ->filter(fn (array $relation, $table) => ($relation['rows'] ?? null) === null && ! $this->isFolded(self::RELATED.'.'.$table))
            ->keys()
            ->all();
    }

    public function fill(string $table, array $rows): void
    {
        $this->related[$table]['rows'] = $rows;

        unset($this->bodies[$table]);
        $this->built = null;

        if (($this->related[$table]['total'] ?? null) === null) {
            $this->related[$table]['shown'] = count($rows);
        }
    }

    private function marker(string $key): string
    {
        return $this->isFolded($key) ? '▸' : '▾';
    }

    /**
     * @param  array<int, array<string, mixed>>  $rows
     * @param  array<int, string>  $hide
     * @return array<int, string>
     */
    /**
     * A single related record, as its fields rather than as a one-row table.
     *
     * @param  array<string, mixed>  $row
     * @param  array<int, string>  $hide
     * @return array<int, string>
     */
    private function record(array $row, array $hide = []): array
    {
        $lines = [];

        foreach ($row as $column => $value) {
            if (in_array((string) $column, $hide, true)) {
                continue;
            }

            $lines[] = $this->field((string) $column, $value, 2);
        }

        return $lines;
    }

    /** Narrow enough to still say something, in a row that cannot fit. */
    private const CELL_FLOOR = 8;

    /** The room a collection row has, once the frame is known. */
    private ?int $width = null;

    private function collection(array $rows, array $hide = []): array
    {
        // The id joining back to this row, and the ids pointing at other
        // tables, are the noise the inspector exists to get rid of. Everything
        // else stays: a column is clamped, never dropped, and i on the row
        // opens the whole value.
        $columns = array_values(array_filter(
            array_keys($rows[0] ?? []),
            fn (string $column) => ! in_array($column, $hide, true),
        ));

        if ($columns === []) {
            return [];
        }

        $natural = [];

        foreach ($columns as $column) {
            $natural[$column] = max(
                mb_strlen((string) $column),
                ...array_map(fn (array $row) => mb_strlen($this->value($row[$column] ?? null)), $rows),
            );
        }

        $cap = $this->cap($natural);

        $widths = array_map(fn (int $width) => min($width, $cap), $natural);

        $lines = [$this->collectionRow(
            array_combine($columns, $columns),
            $columns,
            $widths,
        )];

        foreach ($rows as $row) {
            $lines[] = $this->collectionRow($row, $columns, $widths);
        }

        return $lines;
    }

    /**
     * @param  array<string, mixed>  $row
     * @param  array<int, string>  $columns
     * @param  array<string, int>  $widths
     */
    private function collectionRow(array $row, array $columns, array $widths): string
    {
        $cells = [];

        foreach ($columns as $column) {
            $value = $this->value($row[$column] ?? null);
            $width = $widths[$column];

            $cells[] = mb_str_pad(
                mb_strlen($value) > $width ? mb_substr($value, 0, $width - 1).'…' : $value,
                $width,
            );
        }

        return rtrim(implode('  ', $cells));
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
            return mb_str_pad(static::clamp($column, 21), 22).$value;
        }

        // A description that runs long would otherwise print straight through
        // the type beside it. i or e opens the whole value.
        return mb_str_pad(static::clamp($column, 21), 22)
            .mb_str_pad(static::clamp($value, 46), 48)
            .$type;
    }

    /**
     * Cut a value to a width, with an ellipsis to say it was cut.
     */
    private static function clamp(string $text, int $width): string
    {
        return mb_strlen($text) <= $width ? $text : mb_substr($text, 0, $width - 1).'…';
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
        $this->built = null;
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

    /**
     * The width the content wants, so the modal is sized to what it holds
     * rather than truncating a collection to a number picked in advance.
     */
    /**
     * How much room a collection row has to work with.
     *
     * Terminals differ, so a column is not capped at some number somebody
     * liked: it is capped at what fits, the same way the grid decides its own
     * column widths. Called once the frame knows how wide the box is.
     */
    public function fitTo(int $width): void
    {
        if ($this->width === max(self::CELL_FLOOR, $width)) {
            return;
        }

        $this->width = max(self::CELL_FLOOR, $width);
        $this->bodies = [];
        $this->built = null;
    }

    /**
     * The widest a column may be, so the row as a whole fits.
     *
     * Take the columns that already fit as they are and share what is left
     * between the ones that do not, which is the same as finding the cap where
     * the total comes out right.
     *
     * @param  array<string, int>  $natural
     */
    private function cap(array $natural): int
    {
        if ($this->width === null || $natural === []) {
            return PHP_INT_MAX;
        }

        $gaps = (count($natural) - 1) * 2;
        $room = max(self::CELL_FLOOR, $this->width - $gaps);

        if (array_sum($natural) <= $room) {
            return PHP_INT_MAX;
        }

        $cap = self::CELL_FLOOR;

        for ($try = max($natural); $try >= self::CELL_FLOOR; $try--) {
            $total = array_sum(array_map(fn (int $w) => min($w, $try), $natural));

            if ($total <= $room) {
                $cap = $try;
                break;
            }
        }

        return $cap;
    }

    public function naturalWidth(): int
    {
        $width = 0;

        foreach ($this->lines() as $line) {
            $width = max($width, mb_strlen($line['text']));
        }

        return $width + 4;
    }

    public function text(): string
    {
        return implode("\n", array_column($this->lines(), 'text'));
    }
}
