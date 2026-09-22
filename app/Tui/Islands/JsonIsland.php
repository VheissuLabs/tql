<?php

namespace App\Tui\Islands;

use App\Tui\Json;

class JsonIsland extends Island
{
    public int $offset = 0;

    private array $lines = [];

    public function __construct(
        private string $column,
        private string $value,
        private Styler $style,
    ) {
        $this->title = $column.'  ·  json';
        $this->lines = explode("\n", Json::pretty($value));
    }

    public function total(): int
    {
        return count($this->lines);
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $this->offset = max(0, min($this->offset, max(0, count($this->lines) - $innerHeight)));

        $gutter = mb_strlen((string) count($this->lines)) + 1;
        $room = $innerWidth - $gutter - 2;

        $out = [];

        foreach (array_slice($this->lines, $this->offset, $innerHeight, true) as $number => $line) {
            $label = $this->style->colour('gutter', str_pad((string) ($number + 1), $gutter, ' ', STR_PAD_LEFT));

            $out[] = ' '.$label.' '.$this->highlight($line, $room);
        }

        return $out;
    }

    private function highlight(string $line, int $width): string
    {
        $rendered = '';
        $used = 0;

        foreach (Json::tokenise($line) as [$type, $text]) {
            $length = mb_strlen($text);

            if ($used + $length > $width) {
                $text = mb_substr($text, 0, max(0, $width - $used));
                $length = mb_strlen($text);
            }

            if ($length === 0) {
                continue;
            }

            $rendered .= $type === 'plain' ? $text : $this->style->colour($type, $text);
            $used += $length;

            if ($used >= $width) {
                break;
            }
        }

        return $rendered;
    }
}
