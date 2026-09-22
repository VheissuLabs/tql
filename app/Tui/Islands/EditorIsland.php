<?php

namespace App\Tui\Islands;

use App\Tui\QueryEditor;

class EditorIsland extends Island
{
    public string $title = 'SQL';

    public function __construct(private QueryEditor $editor) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->editor->lines();
        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $start = max(0, $cursorLine - $innerHeight + 1);

        $out = [];

        foreach (array_slice($lines, $start, $innerHeight) as $index => $line) {
            if ($start + $index === $cursorLine) {
                $line = mb_substr($line, 0, $cursorColumn).'█'.mb_substr($line, $cursorColumn);
            }

            $out[] = mb_substr($line, 0, $innerWidth);
        }

        return $out;
    }
}
