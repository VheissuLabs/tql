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

    /**
     * @return array<int, string>
     */
    private function lines(int $width): array
    {
        $lines = [''];

        $name = 2;

        foreach ($this->columns as $column) {
            $name = max($name, mb_strlen((string) ($column['name'] ?? '')));
        }

        $name = min($name, 28);

        foreach ($this->columns as $column) {
            $lines[] = '  '.$this->column($column, $name, $width - $name - 4);
        }

        foreach ($this->indexSection() as $line) {
            $lines[] = $line;
        }

        return $lines;
    }

    /**
     * @param  array<string, mixed>  $column
     */
    private function column(array $column, int $name, int $width): string
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

        return $this->style->pad($this->style->truncate($label, $name), $name)
            .'  '.$this->style->color('keyword', $this->style->pad((string) ($column['type_name'] ?? '?'), 12))
            .'  '.$this->style->dim($this->style->truncate(implode('  ·  ', $notes), max(4, $width)));
    }

    /**
     * @return array<int, string>
     */
    private function indexSection(): array
    {
        if ($this->indexes === []) {
            return [];
        }

        $lines = ['', '  '.$this->style->bold('indexes')];

        foreach ($this->indexes as $index) {
            $columns = implode(', ', $index['columns'] ?? []);
            $notes = [];

            if ($index['primary'] ?? false) {
                $notes[] = 'primary';
            }

            if ($index['unique'] ?? false) {
                $notes[] = 'unique';
            }

            $lines[] = '    '.$this->style->dim(
                ($index['name'] ?: $columns).'  ('.$columns.')'
                .($notes === [] ? '' : '  '.implode(' ', $notes))
            );
        }

        return $lines;
    }
}
