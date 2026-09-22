<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    /**
     * The tag decides the color now, so a per-connection color was a second
     * way to say the same thing.
     */
    public function up(): void
    {
        if (! Schema::hasColumn('connections', 'color')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->dropColumn('color');
        });
    }

    public function down(): void
    {
        if (Schema::hasColumn('connections', 'color')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->string('color')->nullable();
        });
    }
};
