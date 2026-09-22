<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        if (! Schema::hasColumn('connections', 'colour') || Schema::hasColumn('connections', 'color')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->renameColumn('colour', 'color');
        });
    }

    public function down(): void
    {
        if (! Schema::hasColumn('connections', 'color') || Schema::hasColumn('connections', 'colour')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->renameColumn('color', 'colour');
        });
    }
};
