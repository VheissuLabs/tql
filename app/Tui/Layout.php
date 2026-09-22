<?php

namespace App\Tui;

class Layout
{
    public const SIDEBAR = 24;

    public static function topMargin(): int
    {
        return max(0, (int) config('tql.ui.top_margin', 1));
    }

    public static function sqlPosition(): string
    {
        return config('tql.ui.sql_position') === 'bottom' ? 'bottom' : 'top';
    }

    public static function sqlAlways(): bool
    {
        return (bool) config('tql.ui.sql_always', false);
    }

    public static function sqlHeight(int $frameHeight): int
    {
        $configured = (int) config('tql.ui.sql_height', 0);

        if ($configured > 0) {
            return max(3, min($configured, $frameHeight - 5));
        }

        return min(10, max(5, intdiv($frameHeight, 3)));
    }

    public static function rowStyle(): string
    {
        return (string) config('tql.ui.row_style', 'marker');
    }

    public static function mouse(): bool
    {
        return (bool) config('tql.ui.mouse', true);
    }

    public static function inspectRelated(): int
    {
        return (int) config('tql.ui.inspect_related', 10);
    }

    public static function doubleClickMs(): int
    {
        return (int) config('tql.ui.double_click_ms', 400);
    }

    public static function mouseRowOffset(): int
    {
        return (int) config('tql.ui.mouse_row_offset', 0);
    }

    public static function mouseColumnOffset(): int
    {
        return (int) config('tql.ui.mouse_column_offset', 0);
    }

    public static function sidebarWidth(): int
    {
        return max(8, (int) config('tql.ui.sidebar_width', self::SIDEBAR));
    }

    public const CHROME = 7;

    public const TOP_BORDER_ROWS = 3;

    public const SIDEBAR_FIRST_COLUMN = 2;

    public const GRID_HEADER_ROWS = 0;

    public static function sidebarColumns(): array
    {
        return [self::SIDEBAR_FIRST_COLUMN, self::SIDEBAR_FIRST_COLUMN + self::SIDEBAR - 1];
    }

    public static function gridFirstColumn(): int
    {
        return self::SIDEBAR_FIRST_COLUMN + self::SIDEBAR + 1;
    }

    public static function inSidebar(int $column): bool
    {
        [$from, $to] = self::sidebarColumns();

        return $column >= $from && $column <= $to;
    }

    public static function inGrid(int $column): bool
    {
        return $column >= self::gridFirstColumn();
    }

    public static function firstBodyRow(int $leadingNewlines): int
    {
        return $leadingNewlines + self::TOP_BORDER_ROWS + 1;
    }

    public static function bodyIndex(int $row, int $firstBodyRow): ?int
    {
        $index = $row - $firstBodyRow;

        return $index < 0 ? null : $index;
    }
}
