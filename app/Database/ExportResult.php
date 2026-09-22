<?php

namespace App\Database;

class ExportResult
{
    public function __construct(
        public readonly string $path,
        public readonly int $rows,
        public readonly int $bytes,
        public readonly int $durationMs,
    ) {}

    public function size(): string
    {
        $units = ['B', 'KB', 'MB', 'GB'];
        $size = $this->bytes;
        $unit = 0;

        while ($size >= 1024 && $unit < count($units) - 1) {
            $size /= 1024;
            $unit++;
        }

        return round($size, $unit === 0 ? 0 : 1).$units[$unit];
    }
}
