<?php

namespace App\Support;

use Phar;

class Executable
{
    public function __construct(public string $sapi, public string $php, public string $path) {}

    public static function running(): self
    {
        $script = Phar::running(false) ?: (string) ($_SERVER['SCRIPT_FILENAME'] ?? '');

        return self::of(PHP_SAPI, PHP_BINARY, realpath($script) ?: $script);
    }

    public static function of(string $sapi, string $php, string $script): self
    {
        return new self($sapi, $php, $script);
    }

    public function standalone(): bool
    {
        return $this->sapi === 'micro';
    }

    public function command(): array
    {
        return $this->standalone()
            ? [$this->path]
            : [$this->php, $this->path];
    }
}
