<?php

namespace App\Tui\Islands;

class ErrorIsland extends Island
{
    public const WIDTH = 76;

    public string $title = 'ERROR';

    /**
     * @param  array<int, string>  $notes  what to try, when there is something
     */
    public function __construct(
        private string $message,
        private Styler $style,
        private array $notes = [],
        private int $offset = 0,
    ) {}

    /**
     * How tall the box wants to be for a given width, so a long message from
     * a database is read rather than guessed at.
     */
    public function rows(int $width): int
    {
        $body = count($this->wrapped($width - 6)) + count($this->notes);

        // A blank row above and below the message, the hint, and the border.
        return $body + ($this->notes === [] ? 5 : 6);
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [''];

        foreach (array_slice($this->wrapped($innerWidth - 4), $this->offset) as $line) {
            $lines[] = '  '.$line;
        }

        if ($this->notes !== []) {
            $lines[] = '';

            foreach ($this->notes as $note) {
                $lines[] = '  '.$this->style->dim($note);
            }
        }

        $lines[] = '';
        $lines[] = '  '.$this->style->dim('y copies it    esc closes');

        return array_slice($lines, 0, $innerHeight);
    }

    /**
     * @return array<int, string>
     */
    private function wrapped(int $width): array
    {
        $lines = [];

        foreach (explode("\n", $this->message) as $paragraph) {
            foreach (explode("\n", wordwrap($paragraph, max(10, $width), "\n", true)) as $line) {
                $lines[] = $line;
            }
        }

        return $lines;
    }
}
