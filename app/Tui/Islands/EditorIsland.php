<?php

namespace App\Tui\Islands;

use App\Tui\QueryEditor;

class EditorIsland extends Island
{
    public string $title = 'SQL';

    public function __construct(
        private QueryEditor $editor,
        private bool $showCursor = true,
        private ?string $running = null,
        private ?Styler $style = null,
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        if (! $this->showCursor && $this->editor->isEmpty() && $this->running !== null) {
            return $this->showRunning($innerWidth, $innerHeight);
        }

        $lines = $this->editor->lines();
        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $start = max(0, $cursorLine - $innerHeight + 1);

        $out = [];

        foreach (array_slice($lines, $start, $innerHeight) as $index => $line) {
            if ($this->showCursor && $start + $index === $cursorLine) {
                $line = mb_substr($line, 0, $cursorColumn).'█'.mb_substr($line, $cursorColumn);
            }

            $out[] = mb_substr($line, 0, $innerWidth);
        }

        return $out;
    }

    private function showRunning(int $innerWidth, int $innerHeight): array
    {
        $dim = fn (string $t) => $this->style?->dim($t) ?? $t;

        $lines = [$dim(' showing')];

        foreach ($this->split((string) $this->running, $innerWidth - 2) as $line) {
            $lines[] = ' '.$line;
        }

        $lines[] = '';
        $lines[] = $dim(' press s to write your own');

        return array_slice($lines, 0, $innerHeight);
    }

    private function split(string $text, int $width): array
    {
        $out = [];

        foreach (explode("\n", $text) as $line) {
            while (mb_strlen($line) > $width) {
                $out[] = mb_substr($line, 0, $width);
                $line = mb_substr($line, $width);
            }

            $out[] = $line;
        }

        return $out;
    }
}
