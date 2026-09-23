<?php

namespace App\Tui\Islands;

use App\Tui\QueryEditor;
use App\Tui\Sql;

class EditorIsland extends Island
{
    public string $title = 'SQL';

    /** First buffer line drawn, so a click can be mapped back through scroll. */
    public int $firstLine = 0;

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

        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $rows = [];
        $cursorRow = 0;

        foreach ($this->editor->lines() as $number => $line) {
            $chars = $this->characters($line);
            $starts = $this->wrap($line, $innerWidth);

            if ($number === $cursorLine && $cursorColumn === count($chars) && count($chars) - end($starts) >= $innerWidth) {
                $starts[] = count($chars);
            }

            foreach ($starts as $at => $from) {
                $to = $starts[$at + 1] ?? count($chars);

                if ($number === $cursorLine && $cursorColumn >= $from && ($cursorColumn < $to || ! isset($starts[$at + 1]))) {
                    $cursorRow = count($rows);
                }

                $rows[] = [$number, $from, array_slice($chars, $from, $to - $from)];
            }
        }

        $start = max(0, $cursorRow - $innerHeight + 1);

        $this->firstLine = $rows[$start][0] ?? 0;
        $this->rows = array_map(fn (array $row) => [$row[0], $row[1]], array_slice($rows, $start, $innerHeight));

        $out = [];

        foreach (array_slice($rows, $start, $innerHeight, true) as $index => [$number, $from, $chars]) {
            $out[] = $this->paintRow(
                $chars,
                $innerWidth,
                $this->showCursor && $index === $cursorRow ? $cursorColumn - $from : null,
            );
        }

        return $out;
    }

    public array $rows = [];

    public function positionAt(int $localRow, int $localColumn): array
    {
        if ($this->rows === []) {
            return [$this->firstLine, max(0, $localColumn)];
        }

        [$line, $from] = $this->rows[max(0, min($localRow, count($this->rows) - 1))];

        return [$line, $from + max(0, $localColumn)];
    }

    private function wrap(string $line, int $width): array
    {
        $length = mb_strlen($line);
        $starts = [0];
        $at = 0;

        while ($length - $at > $width) {
            $space = mb_strrpos(mb_substr($line, $at, $width), ' ');
            $at += $space === false || $space === 0 ? $width : $space + 1;
            $starts[] = $at;
        }

        return $starts;
    }

    private function paintRow(array $chars, int $width, ?int $cursor): string
    {
        $inverse = fn (string $t) => $this->style?->color('cursor', $t) ?? $t;
        $paint = fn (string $type, string $t) => $this->style?->color($type, $t) ?? $t;

        $rendered = '';

        foreach ($chars as $index => [$type, $char]) {
            $rendered .= $index === $cursor
                ? $inverse($char)
                : ($type === 'plain' ? $char : $paint($type, $char));
        }

        if ($cursor !== null && $cursor >= count($chars) && count($chars) < $width) {
            $rendered .= $inverse(' ');
        }

        return $rendered;
    }

    private function highlight(string $line, int $width, ?int $cursor): string
    {
        $inverse = fn (string $t) => $this->style?->color('cursor', $t) ?? $t;
        $paint = fn (string $type, string $t) => $this->style?->color($type, $t) ?? $t;

        $chars = $this->characters($line);

        // Scroll the line so the caret is always on screen. Without this a
        // statement that exactly fills the pane pushes its own cursor off
        // the right edge and it disappears.
        $shift = $cursor === null ? 0 : max(0, $cursor - $width + 1);

        $rendered = '';
        $used = 0;

        foreach (array_slice($chars, $shift, $width) as $index => [$type, $char]) {
            $rendered .= $shift + $index === $cursor
                ? $inverse($char)
                : ($type === 'plain' ? $char : $paint($type, $char));

            $used++;
        }

        if ($cursor !== null && $cursor >= $shift + $used && $used < $width) {
            $rendered .= $inverse(' ');
        }

        return $rendered;
    }

    /**
     * @return array<int, array{0: string, 1: string}>
     */
    private function characters(string $line): array
    {
        $chars = [];

        foreach (Sql::tokenise($line) as [$type, $text]) {
            foreach (preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [] as $char) {
                $chars[] = [$type, $char];
            }
        }

        return $chars;
    }

    private function showRunning(int $innerWidth, int $innerHeight): array
    {
        $lines = [];

        foreach ($this->split((string) $this->running, $innerWidth) as $line) {
            $lines[] = $this->highlight($line, $innerWidth, null);
        }

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
