<?php

namespace App\Tui;

class Theme
{
    public const COLOURS = [
        'dim', 'default', 'black', 'red', 'green', 'yellow',
        'blue', 'magenta', 'cyan', 'white', 'gray',
    ];

    public static function border(bool $focused): string
    {
        return static::colour($focused ? 'focus_border' : 'border');
    }

    public static function title(bool $focused): string
    {
        return $focused ? static::colour('focus_title') : static::colour('border');
    }

    public static function grid(): string
    {
        return static::colour('grid');
    }

    public static function colour(string $key): string
    {
        $value = (string) config('dotsql.theme.'.$key, 'dim');

        return in_array($value, self::COLOURS, true) ? $value : 'dim';
    }
}
