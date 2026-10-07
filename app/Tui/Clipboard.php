<?php

namespace App\Tui;

class Clipboard
{
    /**
     * What was copied, instead of copying it, while a test has set this to an
     * array. A test run should not write over whatever you last copied.
     *
     * @var array<int, string>|null
     */
    public static ?array $fake = null;

    public static function copy(string $text): string
    {
        if (static::$fake !== null) {
            static::$fake[] = $text;

            return 'fake';
        }

        if ($command = static::command()) {
            $process = proc_open($command, [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);

            if (is_resource($process)) {
                fwrite($pipes[0], $text);
                fclose($pipes[0]);
                fclose($pipes[1]);
                fclose($pipes[2]);

                if (proc_close($process) === 0) {
                    return basename(explode(' ', $command)[0]);
                }
            }
        }

        static::osc52($text);

        return 'terminal';
    }

    public static function osc52(string $text): void
    {
        $payload = base64_encode($text);

        fwrite(STDOUT, "\e]52;c;".$payload."\a");
    }

    private static function command(): ?string
    {
        foreach (['pbcopy', 'wl-copy', 'xclip -selection clipboard', 'xsel --clipboard --input'] as $candidate) {
            $binary = explode(' ', $candidate)[0];

            exec('command -v '.escapeshellarg($binary).' 2>/dev/null', $output, $status);

            if ($status === 0 && $output !== []) {
                return $candidate;
            }
        }

        return null;
    }
}
