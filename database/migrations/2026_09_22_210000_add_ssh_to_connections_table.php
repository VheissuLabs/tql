<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('connections', function (Blueprint $table) {
            $table->string('ssh_host')->nullable();
            $table->integer('ssh_port')->nullable();
            $table->string('ssh_user')->nullable();
            $table->string('ssh_key')->nullable();
        });
    }

    public function down(): void
    {
        Schema::table('connections', function (Blueprint $table) {
            $table->dropColumn(['ssh_host', 'ssh_port', 'ssh_user', 'ssh_key']);
        });
    }
};
