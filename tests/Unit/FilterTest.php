<?php

use App\Database\Filter;
use App\Database\Filters;
use App\Tui\Islands\Island;
use App\Tui\Islands\Screen;
use Illuminate\Database\Query\Grammars\SQLiteGrammar;

function grammar(): SQLiteGrammar
{
    return app('db')->connection()->getQueryGrammar();
}

it('builds each operator', function (string $operator, string $value, string $clause, array $bindings) {
    $built = (new Filter('name', $operator, $value))->toSql(grammar());

    expect($built[0])->toBe($clause)
        ->and($built[1])->toBe($bindings);
})->with([
    ['is', 'karl', '"name" = ?', ['karl']],
    ['is not', 'karl', '"name" != ?', ['karl']],
    ['contains', 'ar', '"name" like ?', ['%ar%']],
    ['starts with', 'ka', '"name" like ?', ['ka%']],
    ['ends with', 'rl', '"name" like ?', ['%rl']],
    ['is greater than', '5', '"name" > ?', ['5']],
    ['is at least', '5', '"name" >= ?', ['5']],
    ['is less than', '5', '"name" < ?', ['5']],
    ['is at most', '5', '"name" <= ?', ['5']],
]);

it('needs no value for empty checks', function () {
    $empty = new Filter('name', 'is empty');

    expect($empty->needsValue())->toBeFalse()
        ->and($empty->usable())->toBeTrue()
        ->and($empty->toSql(grammar()))->toBe(['"name" is null', []]);

    expect((new Filter('name', 'is not empty'))->toSql(grammar()))
        ->toBe(['"name" is not null', []]);
});

it('splits a list for is one of', function () {
    expect((new Filter('id', 'is one of', '1, 2,3 '))->toSql(grammar()))
        ->toBe(['"id" in (?, ?, ?)', ['1', '2', '3']]);
});

it('binds the value rather than putting it in the sql', function () {
    $built = (new Filter('name', 'is', "o'brien; drop table users"))->toSql(grammar());

    expect($built[0])->toBe('"name" = ?')
        ->and($built[1])->toBe(["o'brien; drop table users"]);
});

it('is not usable without a value', function () {
    expect((new Filter('name', 'is', ''))->usable())->toBeFalse()
        ->and((new Filter('', 'is', 'karl'))->usable())->toBeFalse()
        ->and((new Filter('name', 'is', 'karl'))->usable())->toBeTrue();
});

it('joins conditions with and', function () {
    $filters = new Filters([
        new Filter('name', 'contains', 'ar'),
        new Filter('age', 'is at least', '18'),
    ]);

    expect($filters->toSql(grammar()))
        ->toBe(['"name" like ? and "age" >= ?', ['%ar%', '18']]);
});

it('joins conditions with or when asked', function () {
    $filters = new Filters([
        new Filter('name', 'is', 'a'),
        new Filter('name', 'is', 'b'),
    ], 'or');

    expect($filters->toSql(grammar())[0])->toBe('"name" = ? or "name" = ?');
});

it('skips conditions that are not finished', function () {
    $filters = new Filters([
        new Filter('name', 'is', ''),
        new Filter('age', 'is', '18'),
    ]);

    expect($filters->toSql(grammar()))->toBe(['"age" = ?', ['18']]);
});

it('is nothing at all when no condition is usable', function () {
    expect((new Filters([new Filter('name', 'is', '')]))->toSql(grammar()))->toBeNull()
        ->and((new Filters)->toSql(grammar()))->toBeNull();
});

it('draws one overlay on top of another', function () {
    $screen = new Screen;

    $under = new class extends Island
    {
        public function content(int $innerWidth, int $innerHeight): array
        {
            return [];
        }
    };
    $under->place(1, 1, 20, 3);

    $over = clone $under;
    $over->place(5, 2, 6, 1);

    $screen->overlay($under)->overlay($over);

    $lines = $screen->compose(4, fn ($island) => $island === $over
        ? ['OVER  ']
        : ['....................', '....................', '....................']);

    expect($lines[1])->toContain('OVER');
});
