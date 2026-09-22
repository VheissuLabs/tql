<?php

namespace App\Providers;

use App\Mcp\Servers\TqlServer;
use App\Support\ConfigFile;
use App\Support\Paths;
use Devium\Toml\Toml;
use Illuminate\Encryption\EncryptionServiceProvider;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\Schema;
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
            'database.default' => 'tql',
            'database.connections.tql' => [
                'driver' => 'sqlite',
                'database' => Paths::ensureDatabase(),
                'prefix' => '',
                'foreign_key_constraints' => true,
            ],
        ]);

        $this->app->register(EncryptionServiceProvider::class);

        $this->writeUserConfig();
        $this->mergeUserConfig();
    }

    private function writeUserConfig(): void
    {
        if (! $this->app->runningInConsole()) {
            return;
        }

        $result = ConfigFile::ensure();

        if ($result['created']) {
            config(['tql.config_notice' => 'wrote '.Paths::configFile()]);

            return;
        }

        if ($result['added'] !== []) {
            config(['tql.config_notice' => count($result['added']).' new setting'.
                (count($result['added']) === 1 ? '' : 's').' added to config.toml: '.implode(', ', $result['added'])]);
        }
    }

    private function mergeUserConfig(): void
    {
        $user = $this->readUserConfig();

        if ($user === []) {
            return;
        }

        $moved = [];

        config(['tql' => array_replace_recursive(config('tql', []), ConfigFile::hoist($user, $moved))]);

        if ($moved !== []) {
            config(['tql.config_notice' => 'read '.implode(', ', $moved).
                ' from the top of config.toml — move them under their [section] to keep them working']);
        }
    }

    private function readUserConfig(): array
    {
        $toml = Paths::configFile();

        if (is_readable($toml)) {
            try {
                $decoded = Toml::decode((string) file_get_contents($toml), true);

                return is_array($decoded) ? $decoded : [];
            } catch (Throwable $e) {
                config(['tql.config_error' => basename($toml).' could not be read: '.$e->getMessage()]);

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
        $this->migrate();

        Mcp::local('tql', TqlServer::class);
    }

    /**
     * Create the store on first run.
     *
     * A released binary is the first thing a new user touches, and nobody is
     * going to run migrate on a database they did not know existed.
     */
    private function migrate(): void
    {
        if (! $this->app->runningInConsole()) {
            return;
        }

        try {
            if (Schema::connection('tql')->hasTable('connections')) {
                return;
            }

            Artisan::call('migrate', ['--force' => true]);
        } catch (Throwable $e) {
            config(['tql.config_error' => 'could not prepare '.Paths::database().': '.$e->getMessage()]);
        }
    }
}
