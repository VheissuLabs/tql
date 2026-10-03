<?php

namespace App\Commands\Concerns;

use App\Database\AccessRefused;
use Closure;
use Symfony\Component\Console\Output\ConsoleOutputInterface;

trait AnswersAgents
{
    protected function answer(Closure $produce, Closure $draw): int
    {
        try {
            $answer = $produce();
        } catch (AccessRefused $refused) {
            return $this->refuse($refused->getMessage());
        }

        if ($this->wantsJson()) {
            $this->line(json_encode($answer, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE));

            return self::SUCCESS;
        }

        $draw($answer);

        return self::SUCCESS;
    }

    protected function refuse(string $message): int
    {
        $output = $this->output->getOutput();

        $output instanceof ConsoleOutputInterface
            ? $output->getErrorOutput()->writeln($message)
            : $this->error($message);

        return self::FAILURE;
    }

    private function wantsJson(): bool
    {
        if ($this->option('json')) {
            return true;
        }

        return ! $this->option('table') && ! (function_exists('stream_isatty') && @stream_isatty(STDOUT));
    }
}
