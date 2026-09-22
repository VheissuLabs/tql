<?php

namespace App\Tui\Islands;

/**
 * The opaque area behind a modal, and the ring around it.
 *
 * Without it the panes show through the gaps between a modal's boxes, and the
 * modal reads as part of the grid rather than as something on top of it. Since
 * it has to be painted anyway, it carries a border of its own in the modal
 * colors, which closes the modal off from the table behind it.
 */
class BackdropIsland extends Island
{
    public function __construct()
    {
        $this->modal = true;
        $this->focused = true;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        return array_fill(0, max(0, $innerHeight), '');
    }
}
