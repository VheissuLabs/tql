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

    /**
     * Clear the frame's own rows, in place.
     *
     * Not \e[H: that is the top of the screen, and the frame does not always
     * start there — inside a multiplexer pane the row above belongs to
     * something else, and homing the cursor walks the whole app up into it.
     * Going up by the taller of the two frames from where the cursor already
     * is clears the stale row a growing frame would otherwise leave behind,
     * without moving the app.
     */
    private function clearIfAsked(): void
    {
        if (! $this->repaint) {
            return;
        }

        $this->repaint = false;

        $previous = $this->prevFrame === '' ? 1 : count(explode(PHP_EOL, $this->prevFrame));
        $next = count(explode(PHP_EOL, $this->renderTheme()));

        $this->moveCursorToColumn(1);
        $this->moveCursorUp(min($this->terminal()->lines(), max($previous, $next)) - 1);
        $this->eraseDown();

        $this->prevFrame = '';
    }
}
