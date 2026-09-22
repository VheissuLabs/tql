<?php

namespace App\Tui\Islands;

use App\Tui\Json;
use App\Tui\QueryEditor;

class ValueEditorIsland extends Island
{
    public function __construct(
        private string $column,
        private QueryEditor $editor,
        private bool $json,
        private Styler $style,
    ) {
        $this->title = $column.($json ? '  ·  json' : '');
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->editor->lines();
        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $gutter = max(2, mb_strlen((string) count($lines)));
        $room = $innerWidth - $gutter - 3;

        $start = max(0, $cursorLine - $innerHeight + 1);

        $out = [];

        foreach (array_slice($lines, $start, $innerHeight, true) as $number => $line) {
            $label = $this->style->colour('gutter', str_pad((string) ($number + 1), $gutter, ' ', STR_PAD_LEFT));

            $body = $number === $cursorLine
                ? mb_substr($line, 0, $cursorColumn).'▏'.mb_substr($line, $cursorColumn)
                : $line;

            $out[] = ' '.$label.' '.$this->render($body, $room);
        }

        return $out;
    }

    private function render(string $line, int $width): string
    {
        if (! $this->json) {
            return mb_substr($line, 0, $width);
        }

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
        }

        return $rendered === '' ? mb_substr($line, 0, $width) : $rendered;
    }
}
