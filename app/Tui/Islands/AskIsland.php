<?php

namespace App\Tui\Islands;

use App\Tui\QueryEditor;

class AskIsland extends Island
{
    public const WIDTH = 68;

    public const ROWS = 5;

    public string $title = 'ASK';

    public function __construct(
        private QueryEditor $editor,
        private Styler $style,
        private ?string $waiting = null,
    ) {}

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [''];

        foreach ($this->wrapped($innerWidth - 4) as $index => $line) {
            $lines[] = '  '.$line;
        }

        while (count($lines) < self::ROWS + 1) {
            $lines[] = '';
        }

        $lines[] = '';
        $lines[] = '  '.$this->style->dim($this->waiting ?? 'ctrl+s asks    esc cancels    ↵ for a new line');

        return array_slice($lines, 0, $innerHeight);
    }

    /**
     * The question as it will be read: wrapped to the box, with the cursor
     * drawn on the character it is sitting on.
     *
     * @return array<int, string>
     */
    private function wrapped(int $width): array
    {
        $text = $this->editor->buffer();
        $cursor = $this->editor->cursor();

        if ($this->waiting !== null) {
            return array_map(
                fn (string $line) => $this->style->dim($line),
                explode("\n", wordwrap($text, $width, "\n", true)),
            );
        }

        $out = [];
        $seen = 0;

        foreach (explode("\n", $text) as $line) {
            foreach (explode("\n", wordwrap($line, $width, "\n", true)) as $piece) {
                $length = mb_strlen($piece);

                $out[] = $cursor >= $seen && $cursor <= $seen + $length
                    ? $this->withCursor($piece, $cursor - $seen)
                    : $piece;

                $seen += $length + 1;
            }
        }

        return $out === [] ? [$this->style->colour('cursor', ' ')] : $out;
    }

    private function withCursor(string $text, int $at): string
    {
        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        if ($at >= count($chars)) {
            return $text.$this->style->colour('cursor', ' ');
        }

        $chars[$at] = $this->style->colour('cursor', $chars[$at]);

        return implode('', $chars);
    }
}
