<?php

namespace App\Tui\Islands;

use App\Tui\Layout;

/**
 * One or more boxes floating over the panes.
 *
 * Every modal in the browser is the same thing: boxes of a shared width,
 * centred in the frame, on an opaque backdrop that stops the panes showing
 * through — and, unless the config says otherwise, ringed by a border of its
 * own. The row inspector is the only one with two boxes, which is why this
 * takes a list rather than an island.
 */
class Modal
{
    /** @var array<int, array{0: Island, 1: int}> */
    private array $boxes = [];

    public function __construct(
        private int $frameWidth,
        private int $top,
        private int $frameHeight,
        private int $boxWidth,
        private int $gap = 1,
    ) {}

    public function add(Island $island, int $height): static
    {
        $this->boxes[] = [$island, $height];

        return $this;
    }

    /**
     * The rows the boxes take together, including the gaps between them.
     */
    public function height(): int
    {
        $heights = array_map(fn (array $box) => $box[1], $this->boxes);

        return array_sum($heights) + max(0, count($this->boxes) - 1) * $this->gap;
    }

    public function onto(Screen $screen): void
    {
        if ($this->boxes === []) {
            return;
        }

        $total = $this->height();

        $x = max(1, (int) (($this->frameWidth - $this->boxWidth) / 2) + 1);
        $y = $this->top + max(0, (int) (($this->frameHeight - $total) / 2));

        $screen->overlay($this->backdrop($x, $y, $total));

        foreach ($this->boxes as [$island, $height]) {
            $island->modal = true;
            $island->place($x, $y, $this->boxWidth, $height);

            $screen->overlay($island);

            $y += $height + $this->gap;
        }
    }

    /**
     * One column of air, then the ring, so the modal's own border and the
     * backdrop's do not end up touching. Kept inside the frame, since a ring
     * with its bottom edge cut off reads as a mistake. Without the ring the
     * backdrop is the padding alone.
     */
    private function backdrop(int $x, int $y, int $total): BackdropIsland
    {
        $ring = Layout::modalRing();

        $pad = $ring ? 3 : 2;
        $margin = $ring ? 2 : 1;

        $first = max($this->top, $y - $margin);
        $last = min($this->top + $this->frameHeight - 1, $y + $total - 1 + $margin);

        return (new BackdropIsland($ring))->place(
            max(1, $x - $pad),
            $first,
            min($this->frameWidth, $this->boxWidth + ($pad * 2)),
            max($total, $last - $first + 1),
        );
    }
}
