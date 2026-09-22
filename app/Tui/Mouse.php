<?php

namespace App\Tui;

class Mouse
{
    public const LEFT = 0;

    public const WHEEL_UP = 64;

    public const WHEEL_DOWN = 65;

    public const DRAG_LEFT = 32;

    public static function parse(string $sequence): ?array
    {
        if (preg_match_all('/\e\[<(\d+);(\d+);(\d+)([Mm])/', $sequence, $all, PREG_SET_ORDER) < 1) {
            return null;
        }

        $matches = end($all);

        return [
            'button' => (int) $matches[1],
            'column' => (int) $matches[2] - Layout::mouseColumnOffset(),
            'row' => (int) $matches[3] - Layout::mouseRowOffset(),
            'raw_column' => (int) $matches[2],
            'raw_row' => (int) $matches[3],
            'pressed' => $matches[4] === 'M',
        ];
    }
}
