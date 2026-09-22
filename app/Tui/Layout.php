<?php

namespace App\Tui;

class Layout
{
    public const SIDEBAR = 24;

    public const CHROME = 7;

    public const FIRST_BODY_ROW = 3;

    public const SIDEBAR_FIRST_COLUMN = 3;

    public const GRID_HEADER_ROWS = 2;

    public static function sidebarColumns(): array
    {
        return [self::SIDEBAR_FIRST_COLUMN, self::SIDEBAR_FIRST_COLUMN + self::SIDEBAR - 1];
    }

    public static function gridFirstColumn(): int
    {
        return self::SIDEBAR_FIRST_COLUMN + self::SIDEBAR + 3;
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

    public static function bodyIndex(int $row): ?int
    {
        $index = $row - self::FIRST_BODY_ROW;

        return $index < 0 ? null : $index;
    }
}
