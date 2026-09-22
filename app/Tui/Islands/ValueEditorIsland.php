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

    private int $firstLine = 0;

    public function lineAt(int $localRow): ?int
    {
        $line = $this->firstLine + $localRow;

        return $line < count($this->editor->lines()) ? $line : null;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = $this->editor->lines();
        $cursorLine = $this->editor->cursorLine();
        $cursorColumn = $this->editor->cursorColumn();

        $gutter = max(2, mb_strlen((string) count($lines)));
        $room = $innerWidth - $gutter - 4;

        $start = max(0, min($cursorLine - intdiv($innerHeight, 2), count($lines) - $innerHeight));
        $start = max(0, $start);

        $this->firstLine = $start;

        $out = [];

        foreach (array_slice($lines, $start, $innerHeight, true) as $number => $line) {
            $label = $this->style->colour('gutter', str_pad((string) ($number + 1), $gutter, ' ', STR_PAD_LEFT));

            $here = $number === $cursorLine
                ? ($this->showCursor ? ' ' : $this->style->bold('▸'))
                : ' ';

            $selected = $this->selection !== []
                && $number >= $this->selection[0]
                && $number <= $this->selection[1];

            if ($selected) {
                $out[] = $here.$label.' '.$this->style->colour('selection',
                    $this->style->pad(mb_substr($line, 0, $room), $room)
                );

                continue;
            }

            $out[] = $here.$label.' '.$this->render(
                $line,
                $room,
                $this->showCursor && $number === $cursorLine ? $cursorColumn : null,
            );
        }

        return $out;
    }

    private function render(string $line, int $width, ?int $cursor): string
    {
        $tokens = $this->json ? Json::tokenise($line) : [['plain', $line]];

        $inverse = fn (string $t) => $this->style->colour('cursor', $t);
        $paint = fn (string $type, string $t) => $this->style->colour($type, $t);

        $rendered = '';
        $used = 0;

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
            $rendered .= $this->style->colour('cursor', ' ');
        }

        return $rendered;
    }
}
