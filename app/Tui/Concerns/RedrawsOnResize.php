<?php

namespace App\Tui\Concerns;

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

                $key = static::terminal()->read();

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
