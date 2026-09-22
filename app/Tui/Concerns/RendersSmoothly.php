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

    private bool $flowControlDisabled = false;

    /**
     * Prompts sets the tty to "-icanon -isig -echo" but leaves ixon on, so the
     * terminal driver eats ctrl+s as XOFF and it never reaches the key loop.
     * The mode Prompts saved was taken before this, so it still restores.
     */
    private function allowCtrlS(): void
    {
        if ($this->flowControlDisabled) {
            return;
        }

        $this->flowControlDisabled = true;

        if (getenv('NO_TTY_SETUP')) {
            return;
        }

        if (function_exists('stream_isatty') && @stream_isatty(STDIN)) {
            @shell_exec('stty -ixon -ixoff < /dev/tty 2>/dev/null');
        }
    }

    private ?string $lastShape = null;

    /**
     * Anything that changes the shape of the screen — opening a modal, folding
     * a box — is a chance for a frame to be left behind, because Prompts
     * erases using the previous frame's line count. Repaint on the change.
     */
    private function shapeChanged(): bool
    {
        $shape = method_exists($this, 'shape') ? (string) $this->shape() : '';

        $changed = $this->lastShape !== null && $this->lastShape !== $shape;

        $this->lastShape = $shape;

        return $changed;
    }

    protected function render(): void
    {
        $this->allowCtrlS();

        $this->terminal()->initDimensions();

        $size = [$this->terminal()->cols(), $this->terminal()->lines()];

        if (($this->lastSize !== null && $this->lastSize !== $size) || $this->shapeChanged()) {
            $this->repaint = true;
        }

        $this->lastSize = $size;

        if (getenv('NO_SYNC_OUTPUT')) {
            $this->clearIfAsked();

            parent::render();

            return;
        }

        // The clear and the redraw go inside the same synchronized block, so
        // the terminal presents them together. Clearing outside it shows the
        // blank screen for a frame, which reads as a jump.
        static::output()->write("\e[?2026h");

        try {
            $this->clearIfAsked();

            parent::render();
        } finally {
            static::output()->write("\e[?2026l");
        }
    }

    private function clearIfAsked(): void
    {
        if (! $this->repaint) {
            return;
        }

        $this->repaint = false;

        // Home, then erase down: the same shape of redraw Prompts does, but
        // from the top rather than from a line count that may be stale.
        static::output()->write("\e[H\e[J");

        $this->prevFrame = '';
    }
}
