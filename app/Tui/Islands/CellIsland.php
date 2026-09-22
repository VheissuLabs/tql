<?php

namespace App\Tui\Islands;

class CellIsland extends Island
{
    public function __construct(
        private string $column,
        private string $value,
        private Styler $style,
    ) {
        $this->title = $column;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [];

        foreach (explode("\n", $this->value) as $paragraph) {
            foreach ($this->wrap($paragraph, $innerWidth - 2) as $line) {
                $lines[] = ' '.$line;
            }
        }

        if ($lines === []) {
            $lines[] = $this->style->dim(' empty');
        }

        return array_slice($lines, 0, $innerHeight);
    }

    public function length(): int
    {
        return mb_strlen($this->value);
    }

    private function wrap(string $text, int $width): array
    {
        if ($text === '') {
            return [''];
        }

        $lines = [];

        while (mb_strlen($text) > $width) {
            $break = mb_strrpos(mb_substr($text, 0, $width + 1), ' ');

            if ($break === false || $break === 0) {
                $break = $width;
            }

            $lines[] = rtrim(mb_substr($text, 0, $break));
            $text = ltrim(mb_substr($text, $break));
        }

        $lines[] = $text;

        return $lines;
    }
}
