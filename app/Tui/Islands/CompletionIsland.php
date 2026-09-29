<?php

namespace App\Tui\Islands;

use App\Tui\Completion;

class CompletionIsland extends Island
{
    public const ROWS = 8;

    public const WIDEST = 40;

    private const KINDS = ['column' => 'col', 'table' => 'table', 'keyword' => 'sql'];

    public function __construct(private Completion $completion, private Styler $style)
    {
        $this->modal = true;
        $this->focused = true;
    }

    public function rows(): int
    {
        return min(count($this->completion->items), self::ROWS) + 2;
    }

    public function columns(): int
    {
        $longest = max(array_map(fn (array $item) => mb_strlen($item['label']), $this->completion->items));

        return min(self::WIDEST, $longest + 10);
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $items = $this->completion->items;
        $start = $this->window($this->completion->index, count($items), $innerHeight);
        $lines = [];

        foreach (array_slice($items, $start, $innerHeight) as $offset => $item) {
            $lines[] = $this->row($item, $innerWidth, $start + $offset === $this->completion->index);
        }

        return $lines;
    }

    private function row(array $item, int $width, bool $here): string
    {
        $kind = self::KINDS[$item['kind']] ?? '';
        $label = $this->style->pad(' '.$this->style->truncate($item['label'], $width - mb_strlen($kind) - 3), $width - mb_strlen($kind) - 1);

        if ($here) {
            return $this->style->color('selection', $label.$kind.' ');
        }

        return $label.$this->style->dim($kind).' ';
    }
}
