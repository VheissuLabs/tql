<?php

use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function recorded(?string $create = null, ?string $seed = null, array $attributes = []): Browser
{
    $path = sys_get_temp_dir().'/tql-form-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec($create ?? "create table films (
        id integer primary key,
        title text not null,
        rating text default 'G',
        meta text,
        created_at datetime default current_timestamp,
        token text default (lower(hex(randomblob(4))))
    )");
    $pdo->exec($seed ?? "insert into films (title, meta) values ('Alien', '{\"year\":1979}')");

    $connection = Connection::create(array_merge([
        'name' => 'form'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ], $attributes));

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));
    formFrame($browser);

    return $browser;
}

function formFrame(Browser $browser): string
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    return preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser));
}

function press(Browser $browser, string ...$keys): void
{
    foreach ($keys as $key) {
        foreach ($key === "\n" || str_starts_with($key, "\e") || strlen($key) === 1 ? [$key] : mb_str_split($key) as $each) {
            $browser->emit('key', $each);
        }
    }
}

function filmsIn(Browser $browser): array
{
    return (new PDO('sqlite:'.$browser->connection->database))
        ->query('select * from films order by id')
        ->fetchAll(PDO::FETCH_ASSOC);
}

it('opens the row in a form with E, on the field you were on', function () {
    $browser = recorded();
    $browser->columnIndex = 2;

    press($browser, 'E');

    $form = $browser->recordForm;

    expect($form)->not->toBeNull()
        ->and($form->adds())->toBeFalse()
        ->and($form->current()['name'])->toBe('rating')
        ->and($form->value('title'))->toBe('Alien');
});

it('leaves e editing one cell', function () {
    $browser = recorded();

    press($browser, 'e');

    expect($browser->recordForm)->toBeNull()
        ->and($browser->mode)->toBe('edit');
});

it('lays each field out as name (type): value', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E');

    $frame = formFrame($browser);

    expect($frame)->toContain('EDIT ROW  ·  films  ·  id 1')
        ->and($frame)->toContain('  title (text): Alien')
        ->and($frame)->toContain('  rating (text): G')
        ->and($frame)->toContain('ctrl+s keep  esc cancel');
});

it('queues only the fields that changed', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E', "\n");
    $browser->recordForm->editor->set('');
    press($browser, 'Aliens', "\n", "\x13");

    expect($browser->recordForm)->toBeNull()
        ->and($browser->pendingEdits)->toBe(['1' => ['title' => 'Aliens']]);

    press($browser, ':', 'w', "\n");

    expect(filmsIn($browser)[0]['title'])->toBe('Aliens');
});

it('says so when nothing changed', function () {
    $browser = recorded();

    press($browser, 'E', "\x13");

    expect($browser->pendingEdits)->toBe([])
        ->and($browser->status)->toBe('nothing changed');
});

it('asks before throwing changes away', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E', "\n", 'x', "\n", "\e");

    expect($browser->recordForm)->not->toBeNull()
        ->and($browser->recordForm->discarding)->toBeTrue()
        ->and(formFrame($browser))->toContain('esc again throws the row away');

    press($browser, "\e");

    expect($browser->recordForm)->toBeNull()
        ->and($browser->pendingEdits)->toBe([]);
});

it('closes at once when nothing changed', function () {
    $browser = recorded();

    press($browser, 'E', "\e");

    expect($browser->recordForm)->toBeNull();
});

it('puts a field back with esc while typing', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E', "\n", 'zzz', "\e");

    expect($browser->recordForm->editor)->toBeNull()
        ->and($browser->recordForm->value('title'))->toBe('Alien');
});

it('resets a changed field with backspace', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E', "\n", 'zzz', "\n", 'k', "\177");

    expect($browser->recordForm->value('title'))->toBe('Alien')
        ->and($browser->recordForm->dirty())->toBeFalse();
});

it('will not edit the key that names the row', function () {
    $browser = recorded();

    press($browser, 'E', 'g', "\n");

    expect($browser->recordForm->editor)->toBeNull()
        ->and($browser->recordForm->error)->toContain('names the row');
});

it('refuses NULL where the column is not null', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E', "\x0e");

    expect($browser->recordForm->value('title'))->toBe('Alien')
        ->and($browser->recordForm->error)->toContain('not null');
});

it('opens json in the value editor and keeps it compact', function () {
    $browser = recorded();
    $browser->columnIndex = 3;

    press($browser, 'E', "\n");

    $form = $browser->recordForm;

    expect($form->expanded)->toBeTrue()
        ->and($form->json)->toBeTrue()
        ->and(formFrame($browser))->toContain('"year": 1979');

    $form->editor->set('{"year": 1986}');
    press($browser, "\n");

    expect($form->value('meta'))->toBe('{"year":1986}');
});

it('will not keep json that does not parse', function () {
    $browser = recorded();
    $browser->columnIndex = 3;

    press($browser, 'E', "\n");
    $browser->recordForm->editor->set('{"year":');
    press($browser, "\n");

    expect($browser->recordForm->editor)->not->toBeNull()
        ->and($browser->recordForm->error)->toContain('not valid json');
});

it('fills plain defaults, marks time defaults now() and leaves expressions to the database', function () {
    $browser = recorded();

    press($browser, 'N');

    $form = $browser->recordForm;
    $hints = array_column($form->fields(), 'hint', 'name');

    expect($form->adds())->toBeTrue()
        ->and($form->value('rating'))->toBe('G')
        ->and($form->value('created_at'))->toBe('now()')
        ->and($form->state('token'))->toBe('untouched')
        ->and($hints['token'])->toBe('default lower(hex(randomblob(4)))')
        ->and($hints['title'])->toBe('required');

    press($browser, "\n", 'Heat', "\n", "\x13");

    expect($browser->pendingInserts[0])->toEqual([
        'title' => 'Heat',
        'rating' => 'G',
        'created_at' => gmdate('Y-m-d H:i:s'),
    ]);

    press($browser, ':', 'w', "\n");

    $written = filmsIn($browser)[1];

    expect($written['title'])->toBe('Heat')
        ->and($written['token'])->toHaveLength(8);
});

it('edits a row that is not written yet, in place', function () {
    $browser = recorded();

    press($browser, 'N', "\n", 'Heat', "\n", "\x13");

    $browser->columnIndex = 1;
    press($browser, 'E');

    expect($browser->recordForm->insertAt)->toBe(0)
        ->and($browser->recordForm->value('title'))->toBe('Heat');

    press($browser, "\n");
    $browser->recordForm->editor->set('');
    press($browser, 'Ran', "\n", "\x13");

    expect($browser->pendingInserts)->toHaveCount(1)
        ->and($browser->pendingInserts[0]['title'])->toBe('Ran')
        ->and($browser->rows[0]['title'])->toBe('Ran');
});

it('moves on with tab and keeps what was typed', function () {
    $browser = recorded();

    press($browser, 'N', "\n", 'Heat', "\t");

    expect($browser->recordForm->editor)->toBeNull()
        ->and($browser->recordForm->value('title'))->toBe('Heat')
        ->and($browser->recordForm->current()['name'])->toBe('rating');
});

it('refuses E on a read-only connection', function () {
    $browser = recorded(attributes: ['read_only' => true]);

    press($browser, 'E');

    expect($browser->recordForm)->toBeNull()
        ->and($browser->status)->toContain('read-only');
});

it('refuses E where there is no key to name the row by', function () {
    $browser = recorded('create table films (title text)', "insert into films values ('Alien')");

    press($browser, 'E');

    expect($browser->recordForm)->toBeNull()
        ->and($browser->status)->toContain('no single-column primary key');
});

it('shows the field you are on with the cursor, not a marker', function () {
    $browser = recorded();
    $browser->columnIndex = 1;

    press($browser, 'E');

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $line = collect(explode("\n", $render->invoke($browser)))
        ->first(fn (string $line) => str_contains(preg_replace('/\e\[[0-9;]*m/', '', $line), 'title (text):'));

    expect(preg_replace('/\e\[[0-9;]*m/', '', $line))->not->toContain('▸')
        ->and($line)->toContain("\e[7m");

    press($browser, "\n");

    $typing = collect(explode("\n", formFrame($browser)))
        ->first(fn (string $line) => str_contains($line, 'title (text):'));

    expect($typing)->not->toContain('▸');
});

it('keeps every field inside the box', function () {
    $browser = recorded(seed: "insert into films (title) values ('".str_repeat('A very long title ', 20)."')");

    press($browser, 'E');

    $lines = explode("\n", formFrame($browser));
    $top = collect($lines)->search(fn (string $line) => str_contains($line, 'EDIT ROW'));
    $right = mb_strrpos($lines[$top], '┐');

    for ($row = $top + 1; ! str_contains($lines[$row], '┘'); $row++) {
        expect(mb_substr($lines[$row], $right, 1))->toBe('│', "row {$row} spills over: {$lines[$row]}");
    }

    expect(implode("\n", $lines))->toContain('… ↗');
});
