<?php

use App\Support\Store;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
});

it('runs a migration that a newer release brought, on a store that already exists', function () {
    Schema::drop('settings');
    DB::table('migrations')->where('migration', 'like', '%create_settings_table')->delete();

    Store::prepare();

    expect(Schema::hasTable('settings'))->toBeTrue();
});

it('leaves an up-to-date store alone', function () {
    DB::enableQueryLog();

    Store::prepare();

    expect(collect(DB::getQueryLog())->pluck('query')->filter(fn (string $query) => str_starts_with(strtolower($query), 'create')))->toBeEmpty();
});
