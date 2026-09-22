<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Throwable;

/**
 * Small things tql remembers between runs — where the last export went, and
 * anything else that is a preference nobody should have to state twice.
 *
 * Not the config file: that is the user's to write, and this is tql's.
 */
class Setting extends Model
{
    protected $primaryKey = 'key';

    protected $keyType = 'string';

    public $incrementing = false;

    protected $guarded = [];

    /**
     * Reading a preference must never be what breaks a command, so a store
     * that predates the table simply has nothing to say.
     */
    public static function read(string $key): ?string
    {
        try {
            return static::find($key)?->value;
        } catch (Throwable) {
            return null;
        }
    }

    public static function write(string $key, ?string $value): void
    {
        try {
            static::updateOrCreate(['key' => $key], ['value' => $value]);
        } catch (Throwable) {
            // Remembering is a courtesy; failing at it is not worth an error.
        }
    }
}
