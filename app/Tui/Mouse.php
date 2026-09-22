<?php

namespace App\Tui;

use Laravel\Prompts\Prompt;

class Mouse
{
    public const LEFT = 0;

    public const WHEEL_UP = 64;

    public const WHEEL_DOWN = 65;

    public static function enable(): void
    {
        if (getenv('NO_MOUSE')) {
            return;
        }

        Prompt::output()->write("\e[?1000h\e[?1006h");
    }

    public static function disable(): void
    {
        if (getenv('NO_MOUSE')) {
            return;
        }

        Prompt::output()->write("\e[?1006l\e[?1000l");
    }

    public static function parse(string $sequence): ?array
    {
        if (preg_match('/\e\[<(\d+);(\d+);(\d+)([Mm])/', $sequence, $matches) !== 1) {
            return null;
        }

        return [
            'button' => (int) $matches[1],
            'column' => (int) $matches[2],
            'row' => (int) $matches[3],
            'pressed' => $matches[4] === 'M',
        ];
    }
}
