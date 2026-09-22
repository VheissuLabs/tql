<?php

namespace App\Tui;

class Theme
{
    public const COLORS = [
        'dim', 'default', 'black', 'red', 'green', 'yellow',
        'blue', 'magenta', 'cyan', 'white', 'gray',
    ];

    public static function border(bool $focused, bool $modal = false): string
    {
        if ($modal) {
            return static::color($focused ? 'modal_focus_border' : 'modal_border', 'gray');
        }

        return static::color($focused ? 'focus_border' : 'border');
    }

    public static function title(bool $focused, bool $modal = false): string
    {
        if ($modal) {
            return static::color($focused ? 'modal_focus_title' : 'modal_title', 'white');
        }

        return $focused ? static::color('focus_title') : static::color('border');
    }

    /**
     * "inherit" ties the interior grid to the pane's border, so a focused
     * table tints as a whole rather than growing a colored outline.
     */
    public static function grid(bool $focused): string
    {
        return config('tql.theme.grid') === 'inherit'
            ? static::border($focused)
            : static::color('grid');
    }

    /**
     * The block you are on: the selected cell, and the caret in an editor.
     */
    public static function cursor(): string
    {
        return static::color('cursor', 'default');
    }

    /**
     * Everything else that is highlighted but not where you are: the selected
     * table, the selected row, and lines picked out in visual mode.
     */
    public static function selection(): string
    {
        return static::color('selection', 'default');
    }

    /**
     * The glyph shown beside a connection name. Configurable because it
     * depends on the terminal font having the codepoint.
     */
    public static function icon(string $driver): string
    {
        $icons = config('tql.icons', []);

        return (string) ($icons[$driver] ?? $icons['default'] ?? '•');
    }

    public static function color(string $key, string $fallback = 'dim'): string
    {
        $value = (string) config('tql.theme.'.$key, $fallback);

        return in_array($value, self::COLORS, true) ? $value : $fallback;
    }
}
