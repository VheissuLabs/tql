<?php

namespace App\Connections;

/**
 * What a connection is, and the color that says so.
 *
 * A fixed set rather than a configurable one: the point of a tag is that
 * "production" looks the same everywhere, in this application and in the
 * next person's screenshot of it.
 */
enum Tag: string
{
    case Production = 'production';
    case Staging = 'staging';
    case Dev = 'dev';
    case Local = 'local';

    public function color(): string
    {
        return match ($this) {
            self::Production => 'red',
            self::Staging => 'yellow',
            self::Dev => 'blue',
            self::Local => 'green',
        };
    }

    public static function colorOf(?string $tag): string
    {
        return self::parse($tag)?->color() ?? '';
    }

    public static function parse(?string $tag): ?self
    {
        $tag = trim((string) $tag);

        return $tag === '' ? null : self::tryFrom(mb_strtolower($tag));
    }

    /** What "no tag" is called in a list, since a blank row is not a choice. */
    public const NONE = 'none';

    /**
     * @return array<int, string>
     */
    public static function choices(): array
    {
        return array_merge([self::NONE], array_column(self::cases(), 'value'));
    }

    /**
     * The value to store for a choice, so picking "none" clears the tag.
     */
    public static function value(string $choice): string
    {
        return $choice === self::NONE ? '' : $choice;
    }
}
