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
     * @param  array<string, array{rows: array<int, array<string, mixed>>, total: ?int, hide: array<int, string>, kind?: string}>  $related
     */
    public function __construct(
        private array $row,
        private array $types = [],
        private array $related = [],
    ) {}

    /**
     * Every line, in order, tagged with the section it belongs to so the two
     * boxes can be drawn from one list and share one cursor.
     *
     * @return array<int, array{text: string, fold: ?string, section: string, heading: bool, column: ?string}>
     */
    public function lines(): array
    {
        $lines = [[
            'text' => 'RECORD  ('.count($this->row).')',
            'fold' => self::RECORD,
            'section' => self::RECORD,
            'heading' => true,
            'column' => null,
        ]];

        if (! $this->isFolded(self::RECORD)) {
            foreach ($this->row as $column => $value) {
                $lines[] = [
                    'text' => '  '.$this->field((string) $column, $value, 1),
                    'fold' => null,
                    'section' => self::RECORD,
                    'heading' => false,
                    'column' => (string) $column,
                ];
            }
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
            $shown = count($relation['rows']);
            $total = $relation['total'];
            $kind = $relation['kind'] ?? 'has many';
            $one = ! str_starts_with($kind, 'has many');

            $count = $one
                ? ''
                : '  '.($total !== null && $total > $shown ? "({$shown} of {$total})" : "({$shown})");

            // Say when the row is wider than what is on screen, so a missing
            // column is something tql told you about rather than a surprise.
            [$showing, $has] = $one
                ? [0, 0]
                : $this->columnsShown($relation['rows'], $relation['hide'] ?? []);

            $columns = $showing !== 0 && $showing < $has ? "  ·  {$showing} of {$has} columns" : '';

            $lines[] = [
                'text' => '  '.$this->marker($key).' '.$table.'  ·  '.$kind.$count.$columns,
                'fold' => $key,
                'section' => self::RELATED,
                'heading' => false,
                'column' => null,
            ];

            if ($this->isFolded($key)) {
                continue;
            }

            // One record reads as a record: the album's artist is a thing with
            // fields, not a table with one row in it. Many read as a
            // collection — one header, then the rows.
            $body = $one && $shown === 1
                ? $this->record($relation['rows'][0], $relation['hide'] ?? [])
                : $this->collection($relation['rows'], $relation['hide'] ?? []);

            foreach ($body as $line) {
                $lines[] = [
                    'text' => '      '.$line,
                    'fold' => null,
                    'section' => self::RELATED,
                    'heading' => false,
                    'column' => null,
                ];
            }
        }

        return $lines;
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

    /** How many columns a related collection shows before it stops. */
    private const COLUMNS = 6;

    /**
     * Which columns of a related table are worth reading across a row.
     *
     * The ids are noise the inspector exists to remove; a column of prose is
     * worse, because it pushes everything else off the side. What is left is
     * capped, because a related record is a glance, not the table.
     *
     * @param  array<int, array<string, mixed>>  $rows
     * @param  array<int, string>  $hide
     * @return array<int, string>
     */
    private function chosenColumns(array $rows, array $hide = []): array
    {
        $columns = array_values(array_filter(
            array_keys($rows[0] ?? []),
            fn (string $column) => ! in_array($column, $hide, true),
        ));

        if (count($columns) <= self::COLUMNS) {
            return $columns;
        }

        $short = array_values(array_filter(
            $columns,
            fn (string $column) => $this->widest($rows, $column) <= 30,
        ));

        // Everything is prose: keep the first few rather than nothing.
        $keep = count($short) >= 2 ? $short : $columns;

        return array_slice($keep, 0, self::COLUMNS);
    }

    /**
     * @param  array<int, array<string, mixed>>  $rows
     */
    private function widest(array $rows, string $column): int
    {
        return max(array_map(
            fn (array $row) => mb_strlen($this->value($row[$column] ?? null)),
            $rows,
        ) ?: [0]);
    }

    /**
     * How many columns the table has that are worth reading, and how many are
     * being shown, so a heading can say when it is holding some back.
     *
     * @param  array<int, array<string, mixed>>  $rows
     * @param  array<int, string>  $hide
     * @return array{0: int, 1: int}
     */
    public function columnsShown(array $rows, array $hide = []): array
    {
        $all = array_values(array_filter(
            array_keys($rows[0] ?? []),
            fn (string $column) => ! in_array($column, $hide, true),
        ));

        return [count($this->chosenColumns($rows, $hide)), count($all)];
    }

    private function collection(array $rows, array $hide = []): array
    {
        $columns = $this->chosenColumns($rows, $hide);

        if ($columns === []) {
            return [];
        }

        $widths = [];

        foreach ($columns as $column) {
            $widths[$column] = min(28, max(
                mb_strlen((string) $column),
                ...array_map(fn (array $row) => mb_strlen($this->value($row[$column] ?? null)), $rows),
            ));
        }

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

            $cells[] = str_pad(
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
            return str_pad(static::clamp($column, 21), 22).$value;
        }

        // A description that runs long would otherwise print straight through
        // the type beside it. i or e opens the whole value.
        return str_pad(static::clamp($column, 21), 22)
            .str_pad(static::clamp($value, 47), 48)
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
