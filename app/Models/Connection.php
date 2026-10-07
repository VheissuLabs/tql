<?php

namespace App\Models;

use App\Ssh\Settings as SshSettings;
use App\Support\Paths;
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
            SshSettings::SECRET => 'encrypted',
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

    /**
     * A database chosen for this session only.
     *
     * It is not an attribute, so saving the connection — which happens every
     * time it is opened, to record last used — cannot persist it by accident.
     */
    public ?string $sessionDatabase = null;

    public function activeDatabase(): ?string
    {
        return $this->sessionDatabase ?? $this->database;
    }

    public function usesSsh(): bool
    {
        return SshSettings::used($this);
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
        $config['database'] = $this->driver === 'sqlite'
            ? Paths::expand((string) $this->activeDatabase())
            : (string) $this->activeDatabase();

        return array_merge($config, $this->sslConfig());
    }

    /**
     * @return array<string, mixed>
     */
    private function sslConfig(): array
    {
        if ($this->driver === 'sqlsrv') {
            return $this->sqlServerEncryption();
        }

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

    private function sqlServerEncryption(): array
    {
        $mode = trim((string) $this->ssl_mode);

        return [
            'encrypt' => $mode === 'disable'
                ? 'no'
                : 'yes',
            'trust_server_certificate' => in_array($mode, ['verify-ca', 'verify-full'], true)
                ? 'false'
                : 'true',
        ];
    }

    /**
     * A connection string that `tql open` reads back into this connection,
     * password and all. SSH and TLS settings have no place in one, so a
     * connection that uses them needs those set again by hand.
     */
    public function connectionString(): string
    {
        if ($this->driver === 'sqlite') {
            return 'sqlite://'.$this->database;
        }

        $credentials = $this->username === null || $this->username === ''
            ? ''
            : rawurlencode($this->username)
                .($this->password === null || $this->password === '' ? '' : ':'.rawurlencode($this->password))
                .'@';

        return "{$this->driver}://{$credentials}{$this->host}:{$this->port}/"
            .rawurlencode((string) $this->database);
    }

    public function describe(): string
    {
        if ($this->driver === 'sqlite') {
            return 'sqlite:'.static::shorten((string) $this->database);
        }

        if ($this->usesSsh()) {
            return "{$this->driver}://{$this->username}@{$this->host}:{$this->port}/{$this->database}"
                .'  '.SshSettings::describe($this);
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
