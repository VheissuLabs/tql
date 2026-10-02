<?php

namespace App\Support;

use Illuminate\Database\Migrations\Migrator;
use Illuminate\Support\Facades\Artisan;

class Store
{
    public static function prepare(): void
    {
        if (! self::behind()) {
            return;
        }

        Artisan::call('migrate', ['--force' => true]);
    }

    private static function behind(): bool
    {
        $migrator = app('migrator');
        assert($migrator instanceof Migrator);

        if (! $migrator->repositoryExists()) {
            return true;
        }

        $shipped = array_keys($migrator->getMigrationFiles([database_path('migrations')]));

        return array_diff($shipped, $migrator->getRepository()->getRan()) !== [];
    }
}
