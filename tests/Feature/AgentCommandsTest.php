<?php

use App\Mcp\Servers\TqlServer;
use App\Mcp\Tools\ListConnectionsTool;
use App\Mcp\Tools\RunQueryTool;
use App\Models\Connection;
use App\Models\QueryExecution;
use App\Support\Argv;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Connection::query()->delete();

    $path = sys_get_temp_dir().'/tql-agent-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table fruit (id integer primary key, name text not null, qty integer)');
    $pdo->exec("insert into fruit (name, qty) values ('cherry', 3), ('apple', 9), ('banana', 1)");

    $this->fruit = Connection::create(['name' => 'orchard', 'driver' => 'sqlite', 'database' => $path]);
});

function agentRun(string $command, array $arguments = []): array
{
    $status = Artisan::call($command, $arguments);

    return [$status, Artisan::output()];
}

function agentJson(string $command, array $arguments = []): array
{
    [$status, $output] = agentRun($command, $arguments + ['--json' => true]);

    expect($status)->toBe(0);

    return json_decode($output, true);
}

it('lists the saved connections', function () {
    expect(array_column(agentJson('connections'), 'name'))->toBe(['orchard']);
});

it('lists the tables of a connection', function () {
    expect(agentJson('tables', ['connection' => 'orchard'])['tables'])->toBe(['fruit']);
});

it('describes a table', function () {
    $described = agentJson('describe', ['connection' => 'orchard', 'table' => 'fruit']);

    expect($described['primary_key'])->toBe('id')
        ->and(array_column($described['columns'], 'name'))->toBe(['id', 'name', 'qty']);
});

it('runs a read-only query and says how many rows came back', function () {
    $result = agentJson('query', ['connection' => 'orchard', 'sql' => 'select name from fruit order by name']);

    expect(array_column($result['results'], 'name'))->toBe(['apple', 'banana', 'cherry'])
        ->and($result['rows'])->toBe(3)
        ->and($result['truncated'])->toBeFalse();
});

it('stops at the limit and says it did', function () {
    $result = agentJson('query', ['connection' => 'orchard', 'sql' => 'select * from fruit', '--limit' => 2]);

    expect($result['results'])->toHaveCount(2)
        ->and($result['truncated'])->toBeTrue();
});

it('refuses a write, with the reason on stderr and a failing exit code', function () {
    [$status, $output] = agentRun('query', ['connection' => 'orchard', 'sql' => 'delete from fruit']);

    expect($status)->toBe(1)
        ->and($output)->toContain('read-only')
        ->and((new PDO('sqlite:'.$this->fruit->database))->query('select count(*) from fruit')->fetchColumn())->toBe(3);
});

it('fails on an unknown connection and names the ones there are', function () {
    [$status, $output] = agentRun('tables', ['connection' => 'nope']);

    expect($status)->toBe(1)
        ->and($output)->toContain('orchard');
});

it('fails with the database\'s own words on a bad query', function () {
    [$status, $output] = agentRun('query', ['connection' => 'orchard', 'sql' => 'select nope from fruit']);

    expect($status)->toBe(1)
        ->and($output)->toContain('nope');
});

it('draws a table for a person when asked', function () {
    [$status, $output] = agentRun('query', ['connection' => 'orchard', 'sql' => 'select name, qty from fruit order by name', '--table' => true]);

    expect($status)->toBe(0)
        ->and($output)->toContain('name')
        ->and($output)->toContain('apple')
        ->and($output)->not->toContain('{');
});

it('records what it ran in the history as cli', function () {
    agentRun('query', ['connection' => 'orchard', 'sql' => 'select 1 as one', '--json' => true]);

    expect(QueryExecution::where('connection_id', $this->fruit->id)->where('source', 'cli')->pluck('statement')->all())
        ->toBe(['select 1 as one']);
});

it('never reads a command name as a database file to open', function (string $command) {
    $directory = sys_get_temp_dir().'/tql-argv-'.uniqid();
    mkdir($directory);
    touch("{$directory}/{$command}");
    $previous = getcwd();

    try {
        chdir($directory);

        expect(Argv::rewrite(['tql', $command, 'orchard']))->toBe(['tql', $command, 'orchard']);
    } finally {
        chdir($previous);
        unlink("{$directory}/{$command}");
        rmdir($directory);
    }
})->with(['connections', 'tables', 'describe', 'query']);

it('gives the MCP tools the same answers as the commands', function () {
    TqlServer::tool(RunQueryTool::class, ['connection' => 'orchard', 'query' => 'select name from fruit order by name'])
        ->assertOk()
        ->assertSee('"apple"');

    TqlServer::tool(RunQueryTool::class, ['connection' => 'orchard', 'query' => 'delete from fruit'])
        ->assertHasErrors();

    TqlServer::tool(ListConnectionsTool::class)
        ->assertOk()
        ->assertSee('orchard');
});
