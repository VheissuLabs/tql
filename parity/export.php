<?php

$tql = getenv('TQL_PATH') ?: getenv('HOME').'/Code/tql';
$fixture = $argv[1];
$out = $argv[2];

$home = sys_get_temp_dir().'/tql-parity-export-'.getmypid();
@mkdir($home, 0777, true);
putenv("XDG_CONFIG_HOME={$home}");

require $tql.'/vendor/autoload.php';
$app = require $tql.'/bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
config(['app.env' => 'testing']);
Illuminate\Support\Facades\Artisan::call('migrate', ['--force' => true]);
config(['tql.ui.export_path' => $out]);

$connection = App\Models\Connection::create(['name' => 'shop', 'driver' => 'sqlite', 'database' => $fixture]);
$exporter = app(App\Database\SqlExporter::class);
$tables = app(App\Database\QueryRunner::class)->tables($connection);

@mkdir($out, 0777, true);
foreach ($tables as $table) {
    $exporter->table($connection, $table, "{$out}/{$table}.sql");
}
$exporter->table($connection, 'customers', "{$out}/customers-limited.sql", 7);
$exporter->tables($connection, $tables, "{$out}/every-table.sql");
$exporter->tables($connection, $tables, "{$out}/every-table-limited.sql", 2);
$exporter->rows($connection, 'results', ['id', 'name', 'notes'], [['1', "it's", null], ['2', 'line'."\n".'break', 'x']], "{$out}/results.sql");

file_put_contents("{$out}/tables.json", json_encode($tables));
