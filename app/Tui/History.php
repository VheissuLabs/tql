<?php

namespace App\Tui;

use App\Models\Connection;
use App\Models\QueryExecution;
use Illuminate\Support\Carbon;

class History
{
    /**
     * Only what someone wrote. The grid's own selects, the updates behind :w
     * and the like are recorded too, but as tui, and none of them is worth
     * running again by hand.
     */
    public const SOURCES = ['editor', 'mcp'];

    public const LIMIT = 200;

    /**
     * @return array<int, array{statement: string, source: string, succeeded: bool, rows: ?int, at: Carbon}>
     */
    public static function of(Connection $connection): array
    {
        $latest = QueryExecution::query()
            ->selectRaw('max(id)')
            ->where('connection_id', $connection->id)
            ->whereIn('source', self::SOURCES)
            ->groupBy('statement');

        return QueryExecution::query()
            ->whereIn('id', $latest)
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

    /**
     * One line of the statement, for a list that has no room for more.
     */
    public static function line(string $statement): string
    {
        return (string) preg_replace('/\s+/', ' ', trim($statement));
    }

    public static function hint(array $run): string
    {
        return implode(' · ', array_filter([
            $run['source'] === 'mcp' ? 'mcp' : null,
            match (true) {
                ! $run['succeeded'] => 'failed',
                $run['rows'] === null => null,
                default => $run['rows'].($run['rows'] === 1 ? ' row' : ' rows'),
            },
            $run['at']?->diffForHumans(short: true),
        ]));
    }
}
