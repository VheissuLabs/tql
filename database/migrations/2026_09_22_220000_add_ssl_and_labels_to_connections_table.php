<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    /**
     * SQLite adds columns one statement at a time, so a migration that fails
     * halfway leaves some of them behind. Adding only what is missing means
     * running it again finishes the job rather than failing on the first one.
     */
    private const COLUMNS = [
        'ssh_password', 'ssl_mode', 'ssl_key', 'ssl_cert', 'ssl_ca', 'colour', 'tag',
    ];

    public function up(): void
    {
        foreach (self::COLUMNS as $column) {
            if (Schema::hasColumn('connections', $column)) {
                continue;
            }

            Schema::table('connections', function (Blueprint $table) use ($column) {
                $table->string($column)->nullable();
            });
        }
    }

    public function down(): void
    {
        foreach (self::COLUMNS as $column) {
            if (! Schema::hasColumn('connections', $column)) {
                continue;
            }

            Schema::table('connections', function (Blueprint $table) use ($column) {
                $table->dropColumn($column);
            });
        }
    }
};
