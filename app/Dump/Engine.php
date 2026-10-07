<?php

namespace App\Dump;

use App\Models\Connection;

interface Engine
{
    public function name(): string;

    public function dumper(): ?string;

    public function loader(): ?string;

    public function install(): string;

    public function dump(Connection $connection, string $directory, DumpOptions $options): Job;

    public function load(Connection $connection, string $directory, LoadOptions $options): Job;

    public function estimatedBytes(Connection $connection, array $tables): ?int;

    public function compresses(): bool;

    public function recognises(string $directory): bool;
}
