<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

class QueryExecution extends Model
{
    protected $guarded = [];

    protected function casts(): array
    {
        return [
            'succeeded' => 'boolean',
            'row_count' => 'integer',
            'duration_ms' => 'integer',
        ];
    }

    public function target(): BelongsTo
    {
        return $this->belongsTo(Connection::class, 'connection_id');
    }
}
