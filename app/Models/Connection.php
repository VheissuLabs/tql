<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

class Connection extends Model
{
    protected $guarded = [];

    protected function casts(): array
    {
        return [
            'password' => 'encrypted',
            'read_only' => 'boolean',
            'last_used_at' => 'datetime',
            'port' => 'integer',
        ];
    }

    public function executions(): HasMany
    {
        return $this->hasMany(QueryExecution::class);
    }

    public function toLaravelConfig(): array
    {
        return array_filter([
            'driver' => $this->driver,
            'host' => $this->host,
            'port' => $this->port,
            'database' => $this->database,
            'username' => $this->username,
            'password' => $this->password,
            'charset' => $this->driver === 'mysql' ? 'utf8mb4' : null,
            'prefix' => '',
        ], fn ($value) => $value !== null);
    }

    public function describe(): string
    {
        if ($this->driver === 'sqlite') {
            return "sqlite:{$this->database}";
        }

        return "{$this->driver}://{$this->username}@{$this->host}:{$this->port}/{$this->database}";
    }
}
