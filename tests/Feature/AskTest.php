<?php

use App\Ai\Ask;
use App\Ai\Providers;
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

    expect($browser->question?->buffer())->toBe('');

    $browser->emit('key', 'how many customers');

    expect($browser->question?->buffer())->toBe('how many customers')
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

    $plain = preg_replace('/\e\[[0-9;]*m/', '', $method->invoke($browser));

    expect($plain)->toContain('ASK')
        ->and($plain)->toContain('count them')
        ->and($plain)->toContain('↵ asks');
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

it('picks whichever provider has a key', function () {
    config(['ai.providers' => [
        'anthropic' => ['key' => ''],
        'openai' => ['key' => 'sk-test'],
        'gemini' => ['key' => ''],
    ]]);
    config(['tql.ai.provider' => 'auto', 'tql.ai.url' => '']);

    expect(Providers::available())->toBe(['openai'])
        ->and(Providers::chosen())->toBe('openai')
        ->and(Providers::model('openai'))->toBe('gpt-5');
});

it('honours a provider you name', function () {
    config(['ai.providers' => [
        'anthropic' => ['key' => 'sk-ant'],
        'openai' => ['key' => 'sk-openai'],
    ]]);
    config(['tql.ai.provider' => 'anthropic', 'tql.ai.url' => '']);

    expect(Providers::chosen())->toBe('anthropic')
        ->and(Providers::model('anthropic'))->toBe('claude-sonnet-5');
});

it('lets you name the model yourself', function () {
    config(['tql.ai.model' => 'claude-opus-5']);

    expect(Providers::model('anthropic'))->toBe('claude-opus-5');
});

it('says which key is missing for the provider you named', function () {
    config(['ai.providers' => ['groq' => ['key' => '']]]);
    config(['tql.ai.provider' => 'groq', 'tql.ai.url' => '']);

    expect(app(Ask::class)->for(asked()->connection, 'count them'))
        ->toContain('GROQ_API_KEY');
});

it('uses a local endpoint when one is configured', function () {
    config(['ai.providers' => ['anthropic' => ['key' => 'sk-ant']]]);
    config([
        'tql.ai.provider' => 'auto',
        'tql.ai.url' => 'http://localhost:1234/v1',
        'tql.ai.model' => 'qwen2.5-coder-7b',
    ]);

    expect(Providers::chosen())->toBe(Providers::LOCAL);

    $local = config('ai.providers.'.Providers::LOCAL);

    expect($local['driver'])->toBe('openai-compatible')
        ->and($local['url'])->toBe('http://localhost:1234/v1')
        ->and($local['models']['text']['default'])->toBe('qwen2.5-coder-7b');
});

it('does not need a key for a local endpoint', function () {
    config(['ai.providers' => []]);
    config(['tql.ai.provider' => 'auto', 'tql.ai.url' => 'http://localhost:1234/v1', 'tql.ai.key' => '']);

    expect(Providers::chosen())->toBe(Providers::LOCAL)
        ->and(config('ai.providers.'.Providers::LOCAL))->not->toHaveKey('key');
});

it('tells you what to do when nothing is configured at all', function () {
    config(['ai.providers' => ['anthropic' => ['key' => '']]]);
    config(['tql.ai.provider' => 'auto', 'tql.ai.url' => '']);

    expect(app(Ask::class)->for(asked()->connection, 'count them'))
        ->toContain('LM Studio');
});

it('sends the question on enter', function () {
    config(['ai.providers' => ['anthropic' => ['key' => '']]]);
    config(['tql.ai.provider' => 'auto', 'tql.ai.url' => '']);

    $browser = asked();

    $browser->emit('key', 'a');
    $browser->emit('key', 'how many customers');
    $browser->emit('key', "\n");

    // No provider, so it reports that rather than asking — but it did send.
    expect($browser->status)->toContain('no model to ask');
});

it('adds a line on shift+enter instead of sending', function () {
    $browser = asked();

    $browser->emit('key', 'a');
    $browser->emit('key', 'first line');
    $browser->emit('key', "\e[13;2u");
    $browser->emit('key', 'second line');

    expect($browser->question?->buffer())->toBe("first line\nsecond line")
        ->and($browser->status)->toBeNull();
});

it('accepts alt+enter for a new line too', function (string $key) {
    $browser = asked();

    $browser->emit('key', 'a');
    $browser->emit('key', 'one');
    $browser->emit('key', $key);
    $browser->emit('key', 'two');

    expect($browser->question?->buffer())->toBe("one\ntwo");
})->with(["\e\r", "\e\n"]);

it('still sends on ctrl+s', function () {
    config(['ai.providers' => ['anthropic' => ['key' => '']]]);
    config(['tql.ai.provider' => 'auto', 'tql.ai.url' => '']);

    $browser = asked();

    $browser->emit('key', 'a');
    $browser->emit('key', 'count them');
    $browser->emit('key', Browser::SAVE);

    expect($browser->status)->toContain('no model to ask');
});

it('uses a local endpoint when the named provider has no key', function () {
    config([
        'tql.ai.provider' => 'anthropic',
        'tql.ai.url' => 'http://localhost:1234/v1',
        'tql.ai.model' => 'qwen2.5-coder-7b-instruct',
        'ai.providers.anthropic.key' => '',
    ]);

    expect(Providers::chosen())->toBe(Providers::LOCAL);
});

it('says both ways out when there is no key and no endpoint', function () {
    config([
        'tql.ai.provider' => 'anthropic',
        'tql.ai.url' => '',
        'ai.providers.anthropic.key' => '',
    ]);

    $missing = new ReflectionMethod(Ask::class, 'missing');

    expect($missing->invoke(app(Ask::class)))
        ->toContain('ANTHROPIC_API_KEY')
        ->toContain('[ai] url');
});
