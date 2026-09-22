<?php

namespace App\Tui\Islands;

use App\Tui\Picker;

class PickerIsland extends Island
{
    public const WIDTH = 40;

    public const ROWS = 9;

    public string $title = 'PICK';

    public function __construct(
        private Picker $picker,
        private Styler $style,
    ) {
        $this->title = $picker->title;
    }

    public function rows(): int
    {
        return min(count($this->picker->options), self::ROWS) + 5;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $width = $innerWidth - 4;

        $lines = [
            '',
            '  '.$this->style->dim('/').$this->withCursor(
                $this->picker->query->buffer(),
                $this->picker->query->cursor(),
            ),
            '',
        ];

        $matches = $this->picker->matches();

        if ($matches === []) {
            $lines[] = '  '.$this->style->dim('nothing matches');

            return array_slice($lines, 0, $innerHeight);
        }

        // Three rows go above the list: a blank, the query, another blank.
        $room = max(1, $innerHeight - 3);
        $start = $this->window($this->picker->index, count($matches), $room);

        foreach (array_slice($matches, $start, $room) as $offset => $option) {
            $label = '  '.$this->style->pad($this->style->truncate($option, $width), $width);

            $lines[] = ($start + $offset) === $this->picker->index
                ? $this->style->color('selection', $label)
                : $this->style->dim($label);
        }

        return array_slice($lines, 0, $innerHeight);
    }

    private function withCursor(string $text, int $at): string
    {
        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        if ($at >= count($chars)) {
            return $text.$this->style->color('cursor', ' ');
        }

        $chars[$at] = $this->style->color('cursor', $chars[$at]);

        return implode('', $chars);
    }
}
