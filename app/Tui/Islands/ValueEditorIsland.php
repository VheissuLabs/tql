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
        private bool $showCursor = true,
        private array $selection = [],
    ) {
        $this->title = $column.($json ? '  ·  json' : '');
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->editor->lines();
        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $gutter = max(2, mb_strlen((string) count($lines)));
        $room = $innerWidth - $gutter - 4;

        $start = max(0, $cursorLine - $innerHeight + 1);

        $out = [];

        foreach (array_slice($lines, $start, $innerHeight, true) as $number => $line) {
            $label = $this->style->colour('gutter', str_pad((string) ($number + 1), $gutter, ' ', STR_PAD_LEFT));

            $here = $number === $cursorLine
                ? ($this->showCursor ? ' ' : $this->style->bold('▸'))
                : ' ';

            $body = $this->render(
                $line,
                $room,
                $this->showCursor && $number === $cursorLine ? $cursorColumn : null,
            );

            $selected = $this->selection !== []
                && $number >= $this->selection[0]
                && $number <= $this->selection[1];

            $out[] = $here.$label.' '.($selected ? $this->style->inverse($this->style->pad($body, $room)) : $body);
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

                $rendered .= $used === $cursor
                    ? $this->style->inverse($this->style->bold($char === ' ' ? ' ' : $char))
                    : $painted;
                $used++;
            }
        }

        if ($cursor !== null && $cursor >= $used && $used < $width) {
            $rendered .= $this->style->inverse(' ');
        }

        return $rendered;
    }
}
