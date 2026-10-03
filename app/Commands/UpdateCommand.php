<?php

namespace App\Commands;

use App\Models\Setting;
use App\Updates\Installation;
use App\Updates\Releases;
use App\Updates\UpdateCheck;
use App\Updates\UpdateFailed;
use App\Updates\Updater;
use App\Updates\Version;
use App\Updates\Versions;
use LaravelZero\Framework\Commands\Command;

class UpdateCommand extends Command
{
    protected $signature = 'update {--background : Check and update quietly, the way tql does once a day by itself}';

    protected $description = 'Update tql to the latest release';

    public function handle(Updater $updater): int
    {
        $current = Version::of((string) config('app.version')) ?? '0.0.0';
        $installation = Installation::running();

        if ($this->option('background')) {
            UpdateCheck::run($current, $installation, $updater);

            return self::SUCCESS;
        }

        Setting::write(UpdateCheck::CHECKED, (string) now()->timestamp);

        $latest = Releases::latest();

        if ($latest === null) {
            $this->error('Could not reach GitHub to see what the latest release is.');

            return self::FAILURE;
        }

        Setting::write(UpdateCheck::LATEST, $latest);

        if (! Version::isNewer($latest, $current)) {
            $this->info("tql {$current} is the latest.");

            return self::SUCCESS;
        }

        if (! $installation->updatesItself()) {
            $this->line("tql {$latest} is out. You have {$current}; ".$installation->upgrade().'.');

            return self::SUCCESS;
        }

        $this->line("Updating tql {$current} to {$latest}…");

        try {
            $updater->install($latest, $installation->path);
        } catch (UpdateFailed $failure) {
            $this->error(ucfirst($failure->getMessage()).'.');

            return self::FAILURE;
        }

        Setting::write(UpdateCheck::INSTALLED, $latest);

        $versions = Versions::forInstalled($installation->path);

        $this->info($versions->switchesSafelyNow() && $versions->activate()
            ? "Updated to {$latest}, checked against its published checksum. The next tql you start is the new one."
            : "Downloaded {$latest} and checked it against its published checksum. It takes over the next time you start tql.");

        return self::SUCCESS;
    }
}
