<?php

namespace App\Tui\Islands;

use App\Tui\RowDocument;

class InspectIsland extends Island
{
    public string $title = 'ROW';

    public function __construct(
        private RowDocument $document,
        private int $cursor,
        private Styler $style,
        private ?array $selection = null,
    ) {}

    public int $start = 0;

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->document->lines();

        $this->start = $this->window($this->cursor, count($lines), $innerHeight);

        $out = [];

        foreach (array_slice($lines, $this->start, $innerHeight) as $offset => $line) {
            $out[] = $this->line($line, $this->start + $offset, $innerWidth);
        }

        return $out;
    }

    /**
     * @param  array{text: string, fold: ?string, depth: int, column: ?string}  $line
     */
    private function line(array $line, int $index, int $width): string
    {
        $text = $this->style->pad($this->style->truncate($line['text'], $width), $width);

        if ($this->selection !== null && $index >= $this->selection[0] && $index <= $this->selection[1]) {
            return $this->style->colour('selection', $text);
        }

        if ($index === $this->cursor) {
            return $this->style->colour('cursor', $text);
        }

        // A heading is the thing you fold, so it carries the weight.
        return $line['fold'] !== null
            ? $this->style->bold($line['text'])
            : $this->style->dim($line['text']);
    }
}
