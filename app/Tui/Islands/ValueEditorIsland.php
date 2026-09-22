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

            $out[] = ' '.$label.' '.$this->render(
                $line,
                $room,
                $number === $cursorLine ? $cursorColumn : null,
            );
        }

        return $out;
    }

    private function render(string $line, int $width, ?int $cursor): string
    {
        $tokens = $this->json ? Json::tokenise($line) : [['plain', $line]];

        $rendered = '';
        $used = 0;

        foreach ($tokens as [$type, $text]) {
            foreach (preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [] as $char) {
                if ($used >= $width) {
                    return $rendered;
                }

                $painted = $type === 'plain' ? $char : $this->style->colour($type, $char);

                $rendered .= $used === $cursor ? $this->style->inverse($char) : $painted;
                $used++;
            }
        }

        if ($cursor !== null && $cursor >= $used && $used < $width) {
            $rendered .= $this->style->inverse(' ');
        }

        return $rendered;
    }
}
