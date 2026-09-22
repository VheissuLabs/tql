<?php

namespace App\Tui\Islands;

class SectionIsland extends Island
{
    public string $title = '';

    /**
     * @param  array<int, array{text: string, fold: ?string, section: string, heading: bool, column: ?string}>  $lines
     *                                                                                                                  keyed by their index in the whole document
     * @param  array{0: int, 1: int}|null  $selection
     */
    public function __construct(
        private array $lines,
        private int $cursor,
        private Styler $style,
        private ?array $selection = null,
    ) {}

    public int $start = 0;

    public function rows(): int
    {
        return count($this->lines);
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $indexes = array_keys($this->lines);
        $at = array_search($this->cursor, $indexes, true);

        $this->start = $this->window($at === false ? 0 : $at, count($indexes), $innerHeight);

        $out = [];

        foreach (array_slice($this->lines, $this->start, $innerHeight, true) as $index => $line) {
            $out[] = $this->line($line, $index, $innerWidth);
        }

        return $out;
    }

    /**
     * @param  array{text: string, fold: ?string, section: string, heading: bool, column: ?string}  $line
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

        // A relation's heading is what folds, so it carries the weight.
        return $line['fold'] !== null
            ? $this->style->bold($line['text'])
            : $this->style->dim($line['text']);
    }
}
