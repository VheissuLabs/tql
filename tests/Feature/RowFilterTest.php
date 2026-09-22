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

    // The modal opens ready to type; step out before replacing the conditions.
    $browser->emit('key', "\e");

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
    $browser->emit('key', "\e");
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
    $browser->emit('key', "\e");

    $form = $browser->filterForm;

    // It opens on the value, ready to type; step back to the column.
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");

    expect($form->cell)->toBe(FilterForm::COLUMN)
        ->and($form->current()->column)->toBe('id');

    $browser->emit('key', "\e[C");

    expect($form->current()->column)->toBe('name');

    $browser->emit('key', "\t");

    expect($form->cell)->toBe(FilterForm::OPERATOR);

    $browser->emit('key', "\e[C");

    expect($form->current()->operator)->toBe('starts with');
});

it('adds and removes conditions', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

    expect($browser->filterForm->conditions)->toHaveCount(1);

    $browser->emit('key', '+');

    expect($browser->filterForm->conditions)->toHaveCount(2);

    $browser->emit('key', '-');

    expect($browser->filterForm->conditions)->toHaveCount(1);
});

it('keeps at least one condition to edit', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', '-');
    $browser->emit('key', '-');

    expect($browser->filterForm->conditions)->toHaveCount(1);
});

it('has no value cell for an operator that takes none', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

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

it('opens a type-to-filter list on the column cell', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");

    $picker = $browser->filterForm->picker;

    expect($picker)->not->toBeNull()
        ->and($picker->title)->toBe('COLUMN')
        ->and($picker->options)->toBe(['id', 'name', 'age', 'city']);
});

it('filters the list as you type and picks on enter', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");
    $browser->emit('key', 'cit');

    expect($browser->filterForm->picker->matches())->toBe(['city']);

    $browser->emit('key', "\n");

    expect($browser->filterForm->picker)->toBeNull()
        ->and($browser->filterForm->current()->column)->toBe('city');
});

it('opens the operator list on the operator cell', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");

    expect($browser->filterForm->picker->title)->toBe('OPERATOR');

    $browser->emit('key', 'empty');

    expect($browser->filterForm->picker->matches())->toBe(['is empty', 'is not empty']);

    $browser->emit('key', "\n");

    expect($browser->filterForm->current()->operator)->toBe('is empty');
});

it('leaves the list alone on escape', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");

    $was = $browser->filterForm->current()->column;

    $browser->emit('key', "\n");
    $browser->emit('key', 'city');
    $browser->emit('key', "\e");

    expect($browser->filterForm->picker)->toBeNull()
        ->and($browser->filterForm->current()->column)->toBe($was);
});

it('wraps around the ends of the list', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");

    $picker = $browser->filterForm->picker;

    expect($picker->selected())->toBe('id');

    $picker->move(-1);

    expect($picker->selected())->toBe('city');

    $picker->move(1);

    expect($picker->selected())->toBe('id');
});

it('goes back to the top when the list changes under you', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");

    $browser->emit('key', "\e[B");
    $browser->emit('key', "\e[B");

    expect($browser->filterForm->picker->index)->toBe(2);

    $browser->emit('key', 'a');

    expect($browser->filterForm->picker->index)->toBe(0)
        ->and($browser->filterForm->picker->selected())->toBe('name');
});

it('says when nothing matches', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\n");
    $browser->emit('key', 'zzz');

    expect($browser->filterForm->picker->matches())->toBe([])
        ->and($browser->filterForm->picker->selected())->toBeNull();

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))->toContain('nothing matches');
});

it('moves back through the cells with shift+tab', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

    $form = $browser->filterForm;

    expect($form->cell)->toBe(FilterForm::VALUE);

    $browser->emit('key', "\e[Z");

    expect($form->cell)->toBe(FilterForm::OPERATOR);

    $browser->emit('key', "\e[Z");

    expect($form->cell)->toBe(FilterForm::COLUMN);
});

it('can get back to the column after typing a value', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', 'Karl');
    $browser->emit('key', "\n");

    expect($browser->filterForm->current()->value)->toBe('Karl');

    $browser->emit('key', "\e[Z");
    $browser->emit('key', "\e[Z");

    expect($browser->filterForm->cell)->toBe(FilterForm::COLUMN);

    // And the column is still changeable from there.
    $browser->emit('key', "\e[C");

    expect($browser->filterForm->current()->column)->toBe('name')
        ->and($browser->filterForm->current()->value)->toBe('Karl');
});

it('clears the filter with escape from the table', function () {
    $browser = filtered();

    apply($browser, new Filter('name', 'contains', 'Karl'));

    expect(names($browser))->toHaveCount(2);

    $browser->emit('key', "\e");

    expect($browser->filters)->toBeNull()
        ->and(names($browser))->toHaveCount(4)
        ->and($browser->status)->toBe('filter cleared');
});

it('does nothing on escape when nothing is filtered', function () {
    $browser = filtered();

    $browser->emit('key', "\e");

    expect($browser->filters)->toBeNull()
        ->and($browser->status)->not->toBe('filter cleared');
});

it('goes back before clearing a filter it arrived with', function () {
    $browser = filtered();

    apply($browser, new Filter('city', 'is', 'Toronto'));

    expect(names($browser))->toBe(['Karl', 'Bo']);

    // A jump replaces the filter; escaping restores the one you had.
    $browser->filterForm = null;

    $browser->emit('key', "\e");

    expect($browser->filters)->toBeNull();
});

it('opens ready to type a value', function () {
    $browser = filtered();

    $browser->emit('key', 'l');

    expect($browser->headers[$browser->columnIndex])->toBe('name');

    $browser->emit('key', 'f');

    $form = $browser->filterForm;

    // Column and operator are guessed; the value never is.
    expect($form->current()->column)->toBe('name')
        ->and($form->current()->operator)->toBe('contains')
        ->and($form->cell)->toBe(FilterForm::VALUE)
        ->and($form->editor)->not->toBeNull();

    $browser->emit('key', 'Karl');
    $browser->emit('key', Browser::SAVE);

    expect(names($browser))->toBe(['Karl', 'Karla']);
});

it('steps back into the form on escape rather than closing', function () {
    $browser = filtered();

    $browser->emit('key', 'f');

    expect($browser->filterForm->editor)->not->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->filterForm)->not->toBeNull()
        ->and($browser->filterForm->editor)->toBeNull();

    $browser->emit('key', "\e");

    expect($browser->filterForm)->toBeNull();
});

it('never cuts through an escape sequence while typing', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', str_repeat('long value ', 8));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    $frame = $render->invoke($browser);

    // Every escape sequence in the frame is complete.
    preg_match_all('/\e\[[0-9;]*[a-zA-Z]?/', $frame, $matches);

    foreach ($matches[0] as $sequence) {
        expect($sequence)->toMatch('/\e\[[0-9;]*[a-zA-Z]$/');
    }

    // And the cursor is still on screen with the text scrolled to it.
    expect($frame)->toContain("\e[7m");
});

it('moves around the form with h and l once escape has left the value', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

    $form = $browser->filterForm;

    expect($form->cell)->toBe(FilterForm::VALUE)
        ->and($form->editor)->toBeNull();

    // h steps back through the cells rather than typing an h into the value.
    $browser->emit('key', 'h');

    expect($form->cell)->toBe(FilterForm::OPERATOR)
        ->and($form->current()->value)->toBe('')
        ->and($form->editor)->toBeNull();

    // On the operator, l cycles it — the arrows and h/l agree there.
    $browser->emit('key', 'l');

    expect($form->current()->operator)->toBe('starts with');
});

it('still types a value that starts with a letter it uses', function () {
    $browser = filtered();

    $browser->emit('key', 'f');
    $browser->emit('key', "\e");

    // Anything that is not one of the form's own keys goes straight in.
    $browser->emit('key', 'K');

    expect($browser->filterForm->editor)->not->toBeNull()
        ->and($browser->filterForm->editor->buffer())->toBe('K');
});
