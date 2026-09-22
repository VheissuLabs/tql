<?php

namespace App\Tui\Islands;

use Closure;

class Styler
{
    public function __construct(
        private Closure $dim,
        private Closure $bold,
        private Closure $inverse,
        private Closure $underline,
        private Closure $truncate,
    ) {}

    public function dim(string $text): string
    {
        return ($this->dim)($text);
    }

    public function bold(string $text): string
    {
        return ($this->bold)($text);
    }

    public function inverse(string $text): string
    {
        return ($this->inverse)($text);
    }

    public function underline(string $text): string
    {
        return ($this->underline)($text);
    }

    public function truncate(string $text, int $width): string
    {
        return ($this->truncate)($text, $width);
    }

    public function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }

    public function visible(string $text): int
    {
        return mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));
    }
}
