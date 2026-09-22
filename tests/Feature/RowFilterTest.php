<?php

use App\Database\Filter;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\FilterForm;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function filtered(): Browser
{
    $path = sys_get_temp_dir().'/tql-rows-'.uniqid().'.sqlite';
    touch($path);

    $pdo = new PDO('sqlite:'.$path);
    $pdo->exec('create table people (id integer primary key, name text, age integer, city text)');
    $pdo->exec('create table widgets (id integer primary key, label text)');
    $pdo->exec("insert into widgets (label) values ('one')");

    foreach ([
        ['Karl', 41, 'Toronto'],
        ['Karla', 29, 'Montreal'],
        ['Bo', 17, 'Toronto'],
        ['Ada', 36, null],
    ] as [$name, $age, $city]) {
        $pdo->prepare('insert into people (name, age, city) values (?, ?, ?)')->execute([$name, $age, $city]);
    }

    $connection = Connection::create([
        'name' => 'rows'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $browser->emit('key', "\n");
    $render->invoke($browser);

    return $browser;
}

function names(Browser $browser): array
{
    return array_column($browser->raw, 'name');
}

function apply(Browser $browser, Filter ...$conditions): void
{
    $browser->emit('key', 'f');

    $browser->filterForm->conditions = array_values($conditions);

    $browser->emit('key', Browser::SAVE);
}

it('filters rows down to the matches', function () {
    $browser = filtered();

    expect(names($browser))->toHaveCount(4);

    apply($browser, new Filter('name', 'contains', 'Karl'));

    expect(names($browser))->toBe(['Karl', 'Karla']);
});

it('combines conditions with and', function () {
    $browser = filtered();

    apply(
        $browser,
        new Filter('city', 'is', 'Toronto'),
        new Filter('age', 'is at least', '18'),
    );

    expect(names($browser))->toBe(['Karl']);
});

it('combines conditions with or', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->filterForm->conditions = [
        new Filter('name', 'is', 'Bo'),
        new Filter('name', 'is', 'Ada'),
    ];
    $browser->filterForm->toggleJoiner();
    $browser->emit('key', Browser::SAVE);

    expect(names($browser))->toBe(['Bo', 'Ada']);
});

it('finds empty values', function () {
    $browser = filtered();

    apply($browser, new Filter('city', 'is empty'));

    expect(names($browser))->toBe(['Ada']);
});

it('shows the where clause in the sql pane', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'contains', 'Karl'));

    expect($browser->lastStatement)->toContain('where')
        ->and($browser->lastStatement)->toContain('"name" like')
        ->and($browser->lastStatement)->toContain("'%Karl%'");
});

it('says the table is filtered', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'is', 'Karl'));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))->toContain('filtered');
});

it('clears the filter on escape', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'is', 'Karl'));

    expect(names($browser))->toBe(['Karl']);

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

    expect($browser->filters)->toBeNull()
        ->and(names($browser))->toHaveCount(4)
        ->and($browser->status)->toBe('filter cleared');
});

it('reopens with the conditions you had', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'is', 'Karl'));

    $browser->emit('key', 'f');

    expect($browser->filterForm->conditions)->toHaveCount(1)
        ->and($browser->filterForm->conditions[0]->column)->toBe('name')
        ->and($browser->filterForm->conditions[0]->value)->toBe('Karl');
});

it('cycles the column and the operator with the arrows', function () {
    $browser = filtered();

    $browser->emit('key', 'f');

    $form = $browser->filterForm;

    expect($form->current()->column)->toBe('id');

    $browser->emit('key', "\e[C");

    expect($form->current()->column)->toBe('name');

    $browser->emit('key', "\t");

    expect($form->cell)->toBe(FilterForm::OPERATOR);

    $browser->emit('key', "\e[C");

    expect($form->current()->operator)->toBe('is not');
});

it('adds and removes conditions', function () {
    $browser = filtered();

    $browser->emit('key', 'f');

    expect($browser->filterForm->conditions)->toHaveCount(1);

    $browser->emit('key', '+');

    expect($browser->filterForm->conditions)->toHaveCount(2);

    $browser->emit('key', '-');

    expect($browser->filterForm->conditions)->toHaveCount(1);
});

it('keeps at least one condition to edit', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', '-');
    $browser->emit('key', '-');

    expect($browser->filterForm->conditions)->toHaveCount(1);
});

it('has no value cell for an operator that takes none', function () {
    $browser = filtered();

    $browser->emit('key', 'f');

    $form = $browser->filterForm;
    $form->current()->operator = 'is empty';

    $form->moveCell(2);

    expect($form->cell)->toBe(FilterForm::OPERATOR);
});

it('forgets the filter when you change table', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'is', 'Karl'));

    expect($browser->filters)->not->toBeNull();

    $browser->focus = 'sidebar';
    $browser->emit('key', 'j');

    // widgets has no "name" column, so carrying the filter over would break.
    expect($browser->currentTable())->toBe('widgets')
        ->and($browser->filters)->toBeNull()
        ->and($browser->raw)->toHaveCount(1);
});

it('drops pending changes when the filter changes', function () {
    $browser = filtered();

    $browser->emit('key', 'd');

    expect($browser->pendingDeletes)->toHaveCount(1);

    apply($browser, new Filter('name', 'is', 'Karl'));

    expect($browser->pendingDeletes)->toBe([]);
});
