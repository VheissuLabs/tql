<?php

namespace App\Tui\Islands;

class StructureIsland extends Island
{
    public const WIDTH = 78;

    public string $title = 'STRUCTURE';

    /**
     * @param  array<int, array<string, mixed>>  $columns
     * @param  array<string, array{table: string, column: string}>  $links
     * @param  array<int, array<string, mixed>>  $indexes
     */
    public function __construct(
        private array $columns,
        private array $links,
        private array $indexes,
        private ?string $primaryKey,
        private Styler $style,
        private int $offset = 0,
    ) {}

    public int $hidden = 0;

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->lines($innerWidth - 4);

        $this->hidden = max(0, count($lines) - $innerHeight);

        $offset = min($this->offset, $this->hidden);

        $visible = array_slice($lines, $offset, $innerHeight);

        if ($this->hidden > 0 && $offset < $this->hidden) {
            $visible[$innerHeight - 1] = '  '.$this->style->dim('j / ↓ for more');
        }

        return $visible;
    }

    public function naturalWidth(): int
    {
        $longest = 0;

        foreach ($this->columns as $column) {
            $longest = max($longest, $this->nameWidth() + $this->typeWidth() + mb_strlen($this->notes($column)) + 4);
        }

        foreach ($this->indexLines() as $line) {
            $longest = max($longest, mb_strlen($line) + 2);
        }

        return max(self::WIDTH, $longest + 6);
    }

    /**
     * @return array<int, string>
     */
    private function lines(int $width): array
    {
        $lines = [''];

        $name = $this->nameWidth();
        $type = $this->typeWidth();

        foreach ($this->columns as $column) {
            $lines[] = '  '.$this->column($column, $name, $type, $width - $name - $type - 4);
        }

        if ($this->indexes !== []) {
            $lines[] = '';
            $lines[] = '  '.$this->style->bold('indexes');

            foreach ($this->indexLines() as $line) {
                $lines[] = '    '.$this->style->dim($this->style->truncate($line, max(4, $width - 2)));
            }
        }

        return $lines;
    }

    private function nameWidth(): int
    {
        $name = 2;

        foreach ($this->columns as $column) {
            $name = max($name, mb_strlen((string) ($column['name'] ?? '')));
        }

        return min($name, 28);
    }

    private function typeWidth(): int
    {
        $type = 12;

        foreach ($this->columns as $column) {
            $type = max($type, mb_strlen((string) ($column['type_name'] ?? '?')));
        }

        return min($type, 24);
    }

    /**
     * @param  array<string, mixed>  $column
     */
    private function column(array $column, int $name, int $type, int $width): string
    {
        $label = (string) ($column['name'] ?? '');

        return $this->style->pad($this->style->truncate($label, $name), $name)
            .'  '.$this->style->color('keyword', $this->style->pad($this->style->truncate((string) ($column['type_name'] ?? '?'), $type), $type))
            .'  '.$this->style->dim($this->style->truncate($this->notes($column), max(4, $width)));
    }

    /**
     * @param  array<string, mixed>  $column
     */
    private function notes(array $column): string
    {
        $label = (string) ($column['name'] ?? '');

        $notes = [];

        if ($label === $this->primaryKey) {
            $notes[] = 'primary key';
        }

        if (isset($this->links[$label])) {
            $notes[] = '→ '.$this->links[$label]['table'].'.'.$this->links[$label]['column'];
        }

        if (($column['nullable'] ?? false) === false) {
            $notes[] = 'not null';
        }

        if (($column['auto_increment'] ?? false) === true) {
            $notes[] = 'auto';
        }

        $default = $column['default'] ?? null;

        if ($default !== null && $default !== '') {
            $notes[] = 'default '.$default;
        }

        return implode('  ·  ', $notes);
    }

    /**
     * @return array<int, string>
     */
    private function indexLines(): array
    {
        $lines = [];

        foreach ($this->indexes as $index) {
            $columns = implode(', ', $index['columns'] ?? []);
            $notes = [];

            if ($index['primary'] ?? false) {
                $notes[] = 'primary';
            }

            if ($index['unique'] ?? false) {
                $notes[] = 'unique';
            }

            $lines[] = ($index['name'] ?: $columns).'  ('.$columns.')'
                .($notes === [] ? '' : '  '.implode(' ', $notes));
        }

        return $lines;
    }
}
