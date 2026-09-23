<?php

namespace App\Tui\Islands;

use App\Tui\Palette;

class PaletteIsland extends Island
{
    public const WIDTH = 64;

    public const ROWS = 12;

    public string $title = 'COMMANDS';

    public function __construct(private Palette $palette, private Styler $style) {}

    public function rows(): int
    {
        return min(max(1, count($this->palette->items())), self::ROWS) + 5;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [
            '',
            '  '.$this->style->dim('›').' '.$this->typing(),
            '',
        ];

        $matches = $this->palette->matches();

        if ($matches === []) {
            $lines[] = '  '.$this->style->dim('nothing matches');

            return array_slice($lines, 0, $innerHeight);
        }

        $room = max(1, $innerHeight - count($lines));
        $start = $this->window($this->palette->index, count($matches), $room);

        foreach (array_slice($matches, $start, $room) as $offset => $item) {
            $lines[] = $this->row($item, $innerWidth, $start + $offset === $this->palette->index);
        }

        return array_slice($lines, 0, $innerHeight);
    }

    private function row(array $item, int $width, bool $here): string
    {
        $hint = $item['hint'];
        $labelRoom = max(4, $width - mb_strlen($hint) - 6);
        $label = $this->style->pad('  '.$this->style->truncate($item['label'], $labelRoom), $width - mb_strlen($hint) - 2);

        if ($here) {
            return $this->style->color('selection', $label.$hint.'  ');
        }

        return $label.$this->style->dim($hint).'  ';
    }

    private function typing(): string
    {
        $text = $this->palette->query->buffer();
        $at = $this->palette->query->cursor();
        $chars = mb_str_split($text);

        if ($at >= count($chars)) {
            return $text.$this->style->color('cursor', ' ');
        }

        $chars[$at] = $this->style->color('cursor', $chars[$at]);

        return implode('', $chars);
    }
}
