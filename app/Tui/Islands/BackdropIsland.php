<?php

namespace App\Tui\Islands;

/**
 * The opaque area behind a modal.
 *
 * Without it the panes show through the gaps between a modal's boxes, and the
 * modal reads as part of the grid rather than as something on top of it.
 */
class BackdropIsland extends Island
{
    public function __construct()
    {
        $this->bare = true;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        return array_fill(0, max(0, $innerHeight), '');
    }
}
