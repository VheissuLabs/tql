<?php

namespace App\Tui\Concerns;

trait HandlesMouse
{
    public function enableMouse(): void
    {
        if (getenv('NO_MOUSE')) {
            return;
        }

        static::output()->write("\e[?1000h\e[?1002h\e[?1006h");
    }

    public function disableMouse(): void
    {
        if (getenv('NO_MOUSE')) {
            return;
        }

        static::output()->write("\e[?1006l\e[?1002l\e[?1000l");
    }
}
