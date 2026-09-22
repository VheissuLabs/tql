<?php

namespace App\Tui\Concerns;

use App\Tui\Layout;

trait HandlesMouse
{
    public function enableMouse(): void
    {
        if (! static::mouseWanted()) {
            return;
        }

        static::output()->write("\e[?1000h\e[?1002h\e[?1006h");
    }

    public function disableMouse(): void
    {
        if (! static::mouseWanted()) {
            return;
        }

        static::output()->write("\e[?1006l\e[?1002l\e[?1000l");
    }

    /**
     * The mouse is on unless it is turned off.
     *
     * Turning it off gives the terminal its own selection and scrolling back,
     * which is what you want when you would rather copy text with the mouse
     * than click cells with it.
     */
    public static function mouseWanted(): bool
    {
        if (getenv('NO_MOUSE')) {
            return false;
        }

        return Layout::mouse();
    }
}
