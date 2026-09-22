<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;
use PDO;

class Connection extends Model
{
    protected $guarded = [];

    protected function casts(): array
    {
        return [
            'password' => 'encrypted',
            'ssh_password' => 'encrypted',
            'read_only' => 'boolean',
            'last_used_at' => 'datetime',
            'port' => 'integer',
            'ssh_port' => 'integer',
        ];
    }

    public function executions(): HasMany
    {
        return $this->hasMany(QueryExecution::class);
    }

    public function usesSsh(): bool
    {
        return $this->driver !== 'sqlite' && trim((string) $this->ssh_host) !== '';
    }

    /**
     * How the driver is told to use TLS. Empty means whatever the driver does
     * by default, which is what most local databases want.
     */
    public const SSL_MODES = ['', 'disable', 'prefer', 'require', 'verify-ca', 'verify-full'];

    public function usesSsl(): bool
    {
        return $this->driver !== 'sqlite' && trim((string) $this->ssl_mode) !== '';
    }

    public function toLaravelConfig(): array
    {
        $config = array_filter([
            'driver' => $this->driver,
            'host' => $this->host,
            'port' => $this->port,
            'username' => $this->username,
            'password' => $this->password,
            'charset' => $this->driver === 'mysql' ? 'utf8mb4' : null,
            'prefix' => '',
        ], fn ($value) => $value !== null);

        // The connectors read $config['database'] directly, so the key has to
        // be there even when the connection string carried no database name.
        $config['database'] = (string) $this->database;

        return array_merge($config, $this->sslConfig());
    }

    /**
     * @return array<string, mixed>
     */
    private function sslConfig(): array
    {
        if (! $this->usesSsl()) {
            return [];
        }

        return $this->driver === 'pgsql'
            ? array_filter([
                'sslmode' => $this->ssl_mode,
                'sslrootcert' => $this->ssl_ca,
                'sslcert' => $this->ssl_cert,
                'sslkey' => $this->ssl_key,
            ])
            : ['options' => array_filter([
                PDO::MYSQL_ATTR_SSL_CA => $this->ssl_ca,
                PDO::MYSQL_ATTR_SSL_CERT => $this->ssl_cert,
                PDO::MYSQL_ATTR_SSL_KEY => $this->ssl_key,
                // verify-full is the only mode that checks the hostname.
                PDO::MYSQL_ATTR_SSL_VERIFY_SERVER_CERT => $this->ssl_mode === 'verify-full',
            ], fn ($value) => $value !== null && $value !== '')];
    }

    public function describe(): string
    {
        if ($this->driver === 'sqlite') {
            return 'sqlite:'.static::shorten((string) $this->database);
        }

        if ($this->usesSsh()) {
            return "{$this->driver}://{$this->username}@{$this->host}:{$this->port}/{$this->database}"
                .'  ssh '.($this->ssh_user ? $this->ssh_user.'@' : '').$this->ssh_host;
        }

        return "{$this->driver}://{$this->username}@{$this->host}:{$this->port}/{$this->database}";
    }

    /**
     * Home-relative paths, so the part that identifies the database is not
     * pushed off the end by /Users/someone.
     */
    private static function shorten(string $path): string
    {
        $home = (string) (getenv('HOME') ?: '');

        return $home !== '' && str_starts_with($path, $home.'/')
            ? '~'.substr($path, strlen($home))
            : $path;
    }
}
