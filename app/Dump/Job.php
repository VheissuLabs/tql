<?php

namespace App\Dump;

use Closure;

class Job
{
    public function __construct(
        public array $command = [],
        public array $environment = [],
        public ?Closure $native = null,
        public ?Closure $progress = null,
        public ?Closure $tables = null,
    ) {}

    public function expectedTables(): int
    {
        try {
            return $this->tables === null
                ? 0
                : (int) ($this->tables)();
        } catch (\Throwable) {
            return 0;
        }
    }

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
