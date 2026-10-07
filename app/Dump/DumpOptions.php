<?php

namespace App\Dump;

class DumpOptions
{
    public function __construct(
        public array $tables = [],
        public int $threads = 4,
        public bool $dataOnly = false,
        public bool $noLock = false,
    ) {}
}
