<?php

namespace App\Tui\Concerns;

use Closure;
use Laravel\Prompts\Key;
use Laravel\Prompts\Support\Result;

trait RedrawsOnResize
{
    public bool $resized = false;

    public function runLoop(callable $callable): mixed
    {
        $watching = function_exists('pcntl_signal') && defined('SIGWINCH');

        if ($watching) {
            pcntl_async_signals(true);
            pcntl_signal(SIGWINCH, function () {
                $this->resized = true;
            });
        }

        try {
            while (true) {
                $read = [STDIN];
                $write = null;
                $except = null;

                $ready = @stream_select($read, $write, $except, 0, $this->pollMicroseconds());

                if ($this->idle($ready === 0)) {
                    $this->render();
                }

                if ($ready !== 1) {
                    continue;
                }

                $key = $this->joinSplitEscape(static::terminal()->read(), $this->keyFollowingAtOnce(...));

                if ($key === '') {
                    continue;
                }

                $result = $callable($key);

                if ($result instanceof Result) {
                    return $result->value;
                }
            }
        } finally {
            if ($watching) {
                pcntl_signal(SIGWINCH, SIG_DFL);
            }
        }
    }

    private function joinSplitEscape(string $key, Closure $following): string
    {
        if ($key !== Key::ESCAPE) {
            return $key;
        }

        return $key.($following() ?? '');
    }

    private function keyFollowingAtOnce(): ?string
    {
        $read = [STDIN];
        $write = null;
        $except = null;

        if (@stream_select($read, $write, $except, 0, $this->escapeWaitMicroseconds()) !== 1) {
            return null;
        }

        return static::terminal()->read();
    }

    protected function escapeWaitMicroseconds(): int
    {
        return 30_000;
    }

    public function idle(bool $timedOut): bool
    {
        $redraw = $this->resized;
        $this->resized = false;

        return $redraw;
    }

    protected function pollMicroseconds(): int
    {
        return 250_000;
    }
}
