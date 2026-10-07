<?php

namespace App\Dump;

class LoadOptions
{
    public function __construct(
        public bool $drop = false,
        public int $threads = 4,
    ) {}
}
