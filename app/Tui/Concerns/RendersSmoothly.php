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

        if (function_exists('stream_isatty') && @stream_isatty(STDIN)) {
            @shell_exec('stty -ixon -ixoff < /dev/tty 2>/dev/null');
        }
    }

    protected function render(): void
    {
        $this->allowCtrlS();

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
