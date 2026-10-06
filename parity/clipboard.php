<?php

namespace App\Tui;

class Clipboard
{
    public static function copy(string $text): string
    {
        return 'pbcopy';
    }

    public static function osc52(string $text): void {}
}
