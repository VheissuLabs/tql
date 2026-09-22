<?php

namespace App\Tui\Concerns;

trait RendersSmoothly
{
    /** @var array{0: int, 1: int}|null */
    private ?array $lastSize = null;

    private bool $repaint = false;

    /**
     * Force the next render to start from a blank screen. Prompts erases the
     * previous frame by counting its lines, so once anything shifts the frame
     * by a row the stale row at the top is never cleaned up.
     */
    public function repaint(): void
    {
        $this->repaint = true;
    }

    protected function render(): void
    {
        $this->terminal()->initDimensions();

        $size = [$this->terminal()->cols(), $this->terminal()->lines()];

        if ($this->lastSize !== null && $this->lastSize !== $size) {
            $this->repaint = true;
        }

        $this->lastSize = $size;

        if ($this->repaint) {
            $this->repaint = false;

            static::output()->write("\e[2J\e[H");

            $this->prevFrame = '';
            $this->state = $this->state === 'initial' ? 'initial' : 'active';
        }

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
