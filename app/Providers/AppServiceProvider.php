<?php

namespace App\Providers;

use App\Mcp\Servers\DotsqlServer;
use App\Support\Paths;
use Devium\Toml\Toml;
use Illuminate\Encryption\EncryptionServiceProvider;
use Illuminate\Support\ServiceProvider;
use Laravel\Mcp\Facades\Mcp;
use Throwable;

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

        $this->mergeUserConfig();
    }

    private function mergeUserConfig(): void
    {
        $user = $this->readUserConfig();

        if ($user === []) {
            return;
        }

        config(['dotsql' => array_replace_recursive(config('dotsql', []), $user)]);
    }

    private function readUserConfig(): array
    {
        $toml = Paths::configFile();

        if (is_readable($toml)) {
            try {
                $decoded = Toml::decode((string) file_get_contents($toml), true);

                return is_array($decoded) ? $decoded : [];
            } catch (Throwable $e) {
                config(['dotsql.config_error' => basename($toml).' could not be read: '.$e->getMessage()]);

                return [];
            }
        }

        $php = Paths::legacyConfigFile();

        if (is_readable($php)) {
            $decoded = require $php;

            return is_array($decoded) ? $decoded : [];
        }

        return [];
    }

    public function boot(): void
    {
        Mcp::local('dotsql', DotsqlServer::class);
    }
}
