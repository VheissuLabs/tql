<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    /**
     * Not "group": that is a keyword in every dialect, and the form already
     * calls its own sections groups.
     */
    public function up(): void
    {
        if (Schema::hasColumn('connections', 'group_name')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->string('group_name')->nullable();
        });
    }

    public function down(): void
    {
        if (! Schema::hasColumn('connections', 'group_name')) {
            return;
        }

        Schema::table('connections', function (Blueprint $table) {
            $table->dropColumn('group_name');
        });
    }
};
