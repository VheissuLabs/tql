<?php

namespace App\Tui;

use App\Models\Connection;
use App\Models\QueryExecution;

class History
{
    public const SOURCES_SOMEONE_TYPED = ['editor', 'mcp'];

    public const LIMIT = 200;

    public static function of(Connection $connection): array
    {
        return QueryExecution::query()
            ->whereIn('id', QueryExecution::query()
                ->selectRaw('max(id)')
                ->where('connection_id', $connection->id)
                ->whereIn('source', self::SOURCES_SOMEONE_TYPED)
                ->groupBy('statement'))
            ->orderByDesc('id')
            ->limit(self::LIMIT)
            ->get()
            ->map(fn (QueryExecution $run) => [
                'statement' => $run->statement,
                'source' => $run->source,
                'succeeded' => $run->succeeded,
                'rows' => $run->row_count,
                'at' => $run->created_at,
            ])
            ->all();
    }

    public static function oneLine(string $statement): string
    {
        return (string) preg_replace('/\s+/', ' ', trim($statement));
    }

    public static function hint(array $run): string
    {
        return implode(' · ', array_filter([
            $run['source'] === 'mcp'
                ? 'mcp'
                : null,
            self::outcome($run),
            $run['at']?->diffForHumans(short: true),
        ]));
    }

    private static function outcome(array $run): ?string
    {
        if (! $run['succeeded']) {
            return 'failed';
        }

        if ($run['rows'] === null) {
            return null;
        }

        return $run['rows'] === 1
            ? '1 row'
            : $run['rows'].' rows';
    }
}
