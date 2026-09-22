<?php

namespace App\Tui\Islands;

use App\Tui\QueryEditor;
use App\Tui\Sql;

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
            $out[] = $this->highlight(
                $line,
                $innerWidth,
                $this->showCursor && $start + $index === $cursorLine ? $cursorColumn : null,
            );
        }

        return $out;
    }

    private function highlight(string $line, int $width, ?int $cursor): string
    {
        $inverse = fn (string $t) => $this->style?->inverse($t) ?? $t;
        $paint = fn (string $type, string $t) => $this->style?->colour($type, $t) ?? $t;

        $rendered = '';
        $used = 0;

        $tokens = Sql::tokenise($line);

        foreach ($tokens as [$type, $text]) {
            $length = mb_strlen($text);

            $containsCursor = $cursor !== null && $cursor >= $used && $cursor < $used + $length;

            if (! $containsCursor) {
                $room = $width - $used;

                if ($room <= 0) {
                    return $rendered;
                }

                if ($length > $room) {
                    $text = mb_substr($text, 0, $room);
                    $length = $room;
                }

                $rendered .= $type === 'plain' ? $text : $paint($type, $text);
                $used += $length;

                continue;
            }

            foreach (preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [] as $char) {
                if ($used >= $width) {
                    return $rendered;
                }

                $rendered .= $used === $cursor
                    ? $inverse($char)
                    : ($type === 'plain' ? $char : $paint($type, $char));

                $used++;
            }
        }

        if ($cursor !== null && $cursor >= $used && $used < $width) {
            $rendered .= $inverse(' ');
        }

        return $rendered;
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
