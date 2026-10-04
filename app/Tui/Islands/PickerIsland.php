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
        return min(count($this->picker->options) + ($this->picker->creates ? 1 : 0), self::ROWS) + 5;
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
        $newOption = $this->picker->newOption();

        if ($matches === [] && $newOption === null) {
            $lines[] = '  '.$this->style->dim('nothing matches');

            return array_slice($lines, 0, $innerHeight);
        }

        // Three rows go above the list: a blank, the query, another blank.
        $room = max(1, $innerHeight - 3);
        $rows = $newOption === null ? $matches : [...$matches, $newOption];
        $start = $this->window($this->picker->index, count($rows), $room);

        foreach (array_slice($rows, $start, $room, true) as $at => $option) {
            $here = $at === $this->picker->index;

            if ($at === count($matches)) {
                $text = $this->style->pad('  + create '.$this->style->truncate($option, $width - 9), $innerWidth);
                $lines[] = $here ? $this->style->color('selection', $text) : $this->style->dim($text);

                continue;
            }

            $color = $this->picker->colorOf($option);

            // Padded to the full inner width, so the highlight runs the whole
            // way across rather than stopping where the word does.
            $text = $this->style->pad(
                '  '.($color === '' ? '  ' : '● ').$this->style->truncate($option, $width - 2),
                $innerWidth,
            );

            // A highlighted row carries no color of its own: the dot's escape
            // would end the highlight right after it.
            if ($here) {
                $lines[] = $this->style->color('selection', $text);

                continue;
            }

            $lines[] = $color === ''
                ? $text
                : '  '.$this->style->color($color, '●').mb_substr($text, 3);
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
