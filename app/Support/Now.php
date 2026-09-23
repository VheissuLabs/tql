<?php

namespace App\Support;

use DateTimeImmutable;
use DateTimeZone;
use Exception;

/**
 * The time, written the way the column wants it.
 *
 * created_at and updated_at are the reason this exists: filling one in by hand
 * means getting a format right that the database will accept, which is a silly
 * thing to make somebody do.
 *
 * UTC by default, because that is what a database column almost always holds
 * and a row written in local time is wrong in a way nobody notices for months.
 * `[ui] time_zone` says otherwise.
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

        return static::moment()->format(match (true) {
            $type === 'date' => 'Y-m-d',
            $type === 'year' => 'Y',
            str_starts_with($type, 'time') && ! str_starts_with($type, 'timestamp') => 'H:i:s',
            default => 'Y-m-d H:i:s',
        });
    }

    /**
     * Now, in the zone tql has been told to write.
     */
    public static function moment(): DateTimeImmutable
    {
        return new DateTimeImmutable('now', new DateTimeZone(static::zone()));
    }

    /**
     * What to call the zone on screen: EDT, UTC, +05:30.
     */
    public static function label(): string
    {
        $moment = static::moment();

        return $moment->format('T');
    }

    private static function zone(): string
    {
        $zone = trim((string) config('tql.ui.time_zone', 'UTC'));

        if ($zone === '') {
            return 'UTC';
        }

        try {
            new DateTimeZone($zone);
        } catch (Exception) {
            return 'UTC';
        }

        return $zone;
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
