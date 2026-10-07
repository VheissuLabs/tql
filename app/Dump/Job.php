<?php

namespace App\Dump;

use Closure;

class Job
{
    public function __construct(
        public array $command = [],
        public array $environment = [],
        public ?Closure $native = null,
    ) {}

    public static function native(Closure $work): self
    {
        return new self(native: $work);
    }

    public function describe(): string
    {
        return implode(' ', array_map(
            fn (string $part) => preg_match('/^[A-Za-z0-9_\/.=:,@%+-]+$/', $part) === 1
                ? $part
                : escapeshellarg($part),
            $this->command,
        ));
    }
}
