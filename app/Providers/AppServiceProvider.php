<?php

namespace App\Providers;

use App\Support\Paths;
use Illuminate\Encryption\EncryptionServiceProvider;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        config([
            'app.key' => Paths::ensureKey(),
            'app.cipher' => 'AES-256-CBC',
            'database.default' => 'dotsql',
            'database.connections.dotsql' => [
                'driver' => 'sqlite',
                'database' => Paths::ensureDatabase(),
                'prefix' => '',
                'foreign_key_constraints' => true,
            ],
        ]);

        $this->app->register(EncryptionServiceProvider::class);
    }
}
