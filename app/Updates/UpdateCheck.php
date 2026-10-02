<?php

namespace App\Updates;

use App\Models\Setting;
use Throwable;

class UpdateCheck
{
    public const CHECKED = 'updates.checked_at';

    public const LATEST = 'updates.latest';

    public const INSTALLED = 'updates.installed';

    public const EVERY_SECONDS = 86_400;

    public static function due(): bool
    {
        if (! config('tql.updates.check', true)) {
            return false;
        }

        return now()->timestamp - (int) Setting::read(self::CHECKED) >= self::EVERY_SECONDS;
    }

    public static function run(string $current, Installation $installation, Updater $updater): ?string
    {
        Setting::write(self::CHECKED, (string) now()->timestamp);

        $latest = Releases::latest();

        if ($latest === null) {
            return null;
        }

        Setting::write(self::LATEST, $latest);

        if (! Version::isNewer($latest, $current)) {
            return null;
        }

        if (! $installation->updatesItself() || ! config('tql.updates.automatic', true)) {
            return self::notice($current, $installation);
        }

        try {
            $updater->install($latest, $installation->path);
        } catch (Throwable $failure) {
            return $failure->getMessage();
        }

        Setting::write(self::INSTALLED, $latest);

        return self::notice($current, $installation);
    }

    public static function notice(string $current, Installation $installation): ?string
    {
        $installed = Setting::read(self::INSTALLED);

        if ($installed !== null && Version::isNewer($installed, $current)) {
            return "tql {$installed} is ready · restart to use it";
        }

        $latest = Setting::read(self::LATEST);

        if ($latest !== null && Version::isNewer($latest, $current)) {
            return "tql {$latest} is out · ".$installation->upgrade();
        }

        return null;
    }
}
