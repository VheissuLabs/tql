<?php

namespace App\Tui;

class Input
{
    /**
     * The typeable text in a chunk read from the terminal.
     *
     * A paste arrives as one burst of characters rather than a key at a time,
     * so anything that only accepts single characters silently drops it. Key
     * sequences start with escape and are not text at all.
     */
    public static function text(string $chunk, bool $newlines = false): string
    {
        if ($chunk === '' || str_starts_with($chunk, "\e")) {
            return '';
        }

        // Paste carries whichever line ending the source had.
        $chunk = str_replace(["\r\n", "\r"], "\n", $chunk);

        if (! $newlines) {
            $chunk = str_replace("\n", ' ', $chunk);
        }

        $keep = $newlines ? "\n\t" : "\t";

        return (string) preg_replace(
            '/[^\P{C}'.preg_quote($keep, '/').']/u',
            '',
            $chunk,
        );
    }

    public static function isText(string $chunk, bool $newlines = false): bool
    {
        return static::text($chunk, $newlines) !== '';
    }
}
