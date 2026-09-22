<?php

use App\Ai\Ask;
use App\Ai\SchemaSummary;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ai.provider' => 'anthropic']);
});

function asked(): Browser
{
    $path = sys_get_temp_dir().'/tql-ask-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table customers (id integer primary key, name text, email text)');
    $pdo->exec('create table invoices (id integer primary key, customer_id integer, total numeric)');
    $pdo->exec("insert into customers (name, email) values ('Karl', 'karl@example.com')");

    $connection = Connection::create([
        'name' => 'ask'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

it('describes the schema without sending any row data', function () {
    $browser = asked();

    $summary = app(SchemaSummary::class)->for($browser->connection, 'customers');

    expect($summary)->toContain('customers (')
        ->and($summary)->toContain('email')
        ->and($summary)->toContain('invoices (')
        ->and($summary)->not->toContain('Karl')
        ->and($summary)->not->toContain('karl@example.com');
});

it('puts the table you are looking at first', function () {
    $browser = asked();

    $summary = app(SchemaSummary::class)->for($browser->connection, 'invoices');

    expect($summary)->toStartWith('invoices (');
});

it('says so when no provider is configured', function () {
    config(['ai.providers.anthropic.key' => '']);

    expect(app(Ask::class)->configured())->toBeFalse();

    $answer = app(Ask::class)->for(asked()->connection, 'how many customers?');

    expect($answer)->toBeString()
        ->and($answer)->toContain('ANTHROPIC_API_KEY');
});

it('types a question without running anything', function () {
    $browser = asked();

    $browser->emit('key', 'a');

    expect($browser->question)->toBe('');

    $browser->emit('key', 'how many customers');

    expect($browser->question)->toBe('how many customers')
        ->and($browser->mode)->toBe('browse');

    $browser->emit('key', "\e");

    expect($browser->question)->toBeNull();
});

it('shows the question as you type it', function () {
    $browser = asked();

    $browser->emit('key', 'a');
    $browser->emit('key', 'count them');

    $method = new ReflectionMethod($browser, 'renderTheme');
    $method->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser)))->toContain('ask count them');
});

it('puts the answer in the editor as comments above the query', function () {
    $browser = asked();

    $annotate = new ReflectionMethod($browser, 'annotate');
    $annotate->setAccessible(true);

    $sql = $annotate->invoke($browser, [
        'query' => "select count(*)\nfrom customers",
        'explanation' => 'Counts every row in the customers table.',
        'notes' => '',
    ]);

    expect($sql)->toContain('-- Counts every row in the customers table.')
        ->and($sql)->toContain("select count(*)\nfrom customers")
        ->and(app(QueryRunner::class)->isReadOnly($sql))->toBeTrue();
});

it('wraps a long explanation so it fits the editor', function () {
    $browser = asked();

    $annotate = new ReflectionMethod($browser, 'annotate');
    $annotate->setAccessible(true);

    $sql = $annotate->invoke($browser, [
        'query' => 'select 1',
        'explanation' => str_repeat('a long explanation that keeps going ', 8),
        'notes' => 'and a note',
    ]);

    foreach (explode("\n", $sql) as $line) {
        expect(mb_strlen($line))->toBeLessThanOrEqual(80);
    }

    expect($sql)->toContain('-- and a note');
});
