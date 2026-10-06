<?php

$tql = getenv('TQL_PATH') ?: getenv('HOME').'/Code/tql';
$fixture = $argv[1];
$scenarios = json_decode(file_get_contents($argv[2]), true);
$out = $argv[3];

$home = sys_get_temp_dir().'/tql-parity-'.getmypid();
@mkdir($home, 0777, true);
putenv("XDG_CONFIG_HOME={$home}");
putenv('NO_ALT_SCREEN=1');
putenv('NO_MOUSE=1');
putenv('NO_TTY_SETUP=1');

require __DIR__.'/clipboard.php';
require $tql.'/vendor/autoload.php';
$app = require $tql.'/bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
config(['app.env' => 'testing']);
$app->instance('env', 'testing');
Illuminate\Support\Facades\Artisan::call('migrate', ['--force' => true]);
config(['tql.config_notice' => null, 'tql.ui.top_margin' => 0]);

$connection = App\Models\Connection::create(['name' => 'shop', 'driver' => 'sqlite', 'database' => $fixture, 'tag' => 'local']);
$connection->forceFill(['last_used_at' => now()->subHours(11)])->save();
foreach ([
    ['name' => 'Cloud - quill', 'driver' => 'mysql', 'host' => 'db-a2cf02c7.example.com', 'port' => 3306, 'database' => 'quill', 'username' => 'wnadmeh2myuakpvs', 'tag' => 'production', 'used' => now()->subHours(2)],
    ['name' => 'Cloud - notarydash_prod_test', 'driver' => 'mysql', 'host' => 'db-a2cc1a3c.example.com', 'port' => 3306, 'database' => 'notarydash', 'username' => 'rrmgbofxo0yzczv7', 'tag' => 'production', 'used' => now()->subDays(8)],
    ['name' => 'MySql Dev', 'driver' => 'mysql', 'host' => '127.0.0.1', 'port' => 3306, 'database' => '', 'username' => 'root', 'tag' => 'local', 'used' => now()->subDays(9)],
    ['name' => 'pg staging', 'driver' => 'pgsql', 'host' => 'pg.test', 'port' => 5432, 'database' => 'app', 'username' => 'me', 'tag' => 'staging', 'read_only' => true, 'password' => 's3cret', 'used' => null],
    ['name' => 'herd', 'driver' => 'mysql', 'host' => '127.0.0.1', 'port' => 3306, 'database' => 'tql_rust_scratch', 'username' => 'root', 'used' => now()->subDays(400)],
    ['name' => 'herd-any', 'driver' => 'mysql', 'host' => '127.0.0.1', 'port' => 3306, 'database' => '', 'username' => 'root', 'used' => now()->subDays(500)],
    ...(getenv('TQL_PARITY_SQLSRV') ? [[
        'name' => 'mssql', 'driver' => 'sqlsrv', 'host' => explode(':', getenv('TQL_PARITY_SQLSRV'))[0],
        'port' => (int) explode(':', getenv('TQL_PARITY_SQLSRV'))[1], 'database' => 'tql_rust_scratch',
        'username' => 'sa', 'password' => getenv('TQL_PARITY_SQLSRV_PASSWORD'), 'used' => now()->subDays(600),
    ]] : []),
    ['name' => 'demo', 'driver' => 'sqlite', 'database' => getenv('HOME').'/Code/tql/demo.db', 'used' => now()->subDays(3)],
] as $attributes) {
    if (str_starts_with($attributes['name'], 'herd') && ! getenv('TQL_PARITY_MYSQL')) {
        continue;
    }
    $used = $attributes['used'];
    unset($attributes['used']);
    App\Models\Connection::create($attributes)->forceFill(['last_used_at' => $used])->save();
}

@mkdir($out, 0777, true);
$pristine = $fixture.'.pristine';
copy($fixture, $pristine);

$defaults = config('tql');
foreach ($scenarios as $scenario) {
    $sqlServer = ($scenario['connection'] ?? null) === 'mssql';
    if (isset($scenario['connection']) && ! getenv($sqlServer ? 'TQL_PARITY_SQLSRV' : 'TQL_PARITY_MYSQL')) {
        continue;
    }
    copy($pristine, $fixture);
    if ($sqlServer) {
        app(App\Database\ConnectionManager::class)
            ->resolve(App\Models\Connection::where('name', 'mssql')->firstOrFail())
            ->unprepared(file_get_contents(__DIR__.'/fixture-sqlsrv.sql'));
    } elseif (isset($scenario['connection'])) {
        $reset = file_get_contents(__DIR__.'/reset-mysql.sql')."use tql_rust_scratch;\n".file_get_contents(__DIR__.'/fixture-mysql-data.sql');
        $process = proc_open(['mysql', '-uroot', '-h127.0.0.1', '-P3306'], [0 => ['pipe', 'r']], $pipes);
        fwrite($pipes[0], $reset);
        fclose($pipes[0]);
        proc_close($process);
    }
    config(['tql' => $defaults]);
    foreach ($scenario['config'] ?? [] as $key => $value) {
        config(['tql.'.$key => $value]);
    }
    App\Keys\Keymap::forget();
    [$cols, $lines] = $scenario['size'];
    putenv("COLUMNS={$cols}");
    $picker = ($scenario['screen'] ?? 'browser') === 'picker';
    $spare = match (true) {
        $picker && ($cols < 60 || $lines < 10), ! $picker && ($cols < 60 || $lines < 12) => 0,
        $picker => 0,
        default => 1,
    };
    putenv('LINES='.($lines + $spare));

    if ($picker) {
        $realHome = getenv('HOME');
        $realCwd = getcwd();
        if ($fakeHome = getenv('TQL_PARITY_HOME')) {
            putenv("HOME={$fakeHome}");
            chdir($fakeHome);
        }
        Illuminate\Support\Facades\DB::beginTransaction();
        try {
            $prompt = new App\Tui\ConnectionPicker(App\Models\Connection::orderByDesc('last_used_at')->orderBy('name')->get());
            $prompt->terminal()->initDimensions();
            foreach ($scenario['keys'] as $key) {
                $prompt->emit('key', $key);
            }
            $frame = (string) (new App\Prompts\Renderers\ConnectionPickerRenderer($prompt))($prompt);
            file_put_contents("{$out}/{$scenario['name']}.txt", preg_replace('/\e\[[0-9;?]*[A-Za-z]/', '', $frame));
        } finally {
            Illuminate\Support\Facades\DB::rollBack();
            putenv("HOME={$realHome}");
            chdir($realCwd);
        }
        continue;
    }

    $browser = new App\Tui\Browser(
        App\Models\Connection::where('name', $scenario['connection'] ?? 'shop')->firstOrFail(),
        app(App\Database\QueryRunner::class),
        app(App\Tui\RowFormatter::class),
    );
    $browser->terminal()->initDimensions();
    (new App\Prompts\Renderers\BrowserRenderer($browser))($browser);

    foreach ($scenario['keys'] as $key) {
        $browser->emit('key', $key);
        while ($browser->idle(true)) {
        }
        (new App\Prompts\Renderers\BrowserRenderer($browser))($browser);
    }

    $frame = (string) (new App\Prompts\Renderers\BrowserRenderer($browser))($browser);
    file_put_contents("{$out}/{$scenario['name']}.txt", preg_replace('/\e\[[0-9;?]*[A-Za-z]/', '', $frame));
}

copy(getenv('XDG_CONFIG_HOME').'/tql/tql.sqlite', "{$out}/tql.sqlite");
copy(getenv('XDG_CONFIG_HOME').'/tql/key', "{$out}/key");
