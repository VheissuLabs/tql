<?php

namespace App\Tui\Concerns;

trait RendersSmoothly
{
    protected function render(): void
    {
        if (getenv('NO_SYNC_OUTPUT')) {
            parent::render();

            return;
        }

        static::output()->write("\e[?2026h");

        try {
            parent::render();
        } finally {
            static::output()->write("\e[?2026l");
        }
    }
}
