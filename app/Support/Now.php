<?php

namespace App\Support;

/**
 * The time, written the way the column wants it.
 *
 * created_at and updated_at are the reason this exists: filling one in by hand
 * means getting a format right that the database will accept, which is a silly
 * thing to make somebody do.
 */
class Now
{
    /** What you type in a cell to mean this. */
    public const LITERAL = 'now()';

    /**
     * Is this what somebody typed to ask for the time?
     */
    public static function asked(string $value): bool
    {
        return strtolower(trim($value)) === self::LITERAL;
    }

    /**
     * The current time, formatted for a column of this type.
     *
     * A date column does not want the hours, a time column does not want the
     * date, and everything else takes the full stamp.
     */
    public static function for(?string $type = null): string
    {
        $type = strtolower(trim((string) $type));

        return date(match (true) {
            $type === 'date' => 'Y-m-d',
            $type === 'year' => 'Y',
            str_starts_with($type, 'time') && ! str_starts_with($type, 'timestamp') => 'H:i:s',
            default => 'Y-m-d H:i:s',
        });
    }

    /**
     * Does this column hold a moment in time?
     *
     * Used to offer the shortcut where it is worth offering, rather than in
     * the middle of editing somebody's surname.
     */
    public static function suits(?string $type): bool
    {
        $type = strtolower(trim((string) $type));

        if ($type === '') {
            return false;
        }

        foreach (['date', 'time', 'year'] as $shape) {
            if (str_contains($type, $shape)) {
                return true;
            }
        }

        return false;
    }
}
