<?php

use App\Tui\Completion;

function complete(string $typed, ?string $open = null, string $after = ''): ?Completion
{
    $columns = [
        'users' => ['id', 'name', 'email', 'created_at'],
        'user_roles' => ['user_id', 'role'],
        'orders' => ['id', 'user_id', 'total', 'placed_at'],
        'Order Items' => ['order_id', 'qty'],
    ];

    return Completion::at(
        $typed.$after,
        mb_strlen($typed),
        array_keys($columns),
        fn (string $table) => $columns[$table] ?? [],
        $open,
        fn (string $name) => '"'.$name.'"',
    );
}

function offered(?Completion $completion): array
{
    return array_column($completion?->items ?? [], 'text');
}

it('offers only tables after from, join, into and update', function (string $typed) {
    expect(offered(complete($typed)))->toBe(['users', 'user_roles']);
})->with([
    'select * from us',
    'select * from orders join us',
    'insert into us',
    'update us',
]);

it('offers the columns of a table named after the cursor', function () {
    expect(offered(complete('select na', after: ' from users'))[0])->toBe('name');
});

it('offers every column of an alias after its dot', function () {
    expect(offered(complete('select u.', after: ' from users u')))->toBe(['id', 'name', 'email', 'created_at']);
});

it('narrows an alias to its own table', function () {
    expect(offered(complete('select o.', after: ' from users u join orders as o on o.user_id = u.id')))
        ->toBe(['id', 'user_id', 'total', 'placed_at']);
});

it('takes a table name before the dot as well as an alias', function () {
    expect(offered(complete('select orders.to', after: ' from orders')))->toBe(['total']);
});

it('falls back to the open table when the statement names none', function () {
    expect(offered(complete('select em', open: 'users')))->toBe(['email']);
});

it('offers columns, then tables, then keywords', function () {
    $completion = complete('select * from orders where u');

    expect(array_slice(offered($completion), 0, 4))->toBe(['user_id', 'users', 'user_roles', 'union'])
        ->and(array_slice(array_column($completion->items, 'kind'), 0, 4))->toBe(['column', 'table', 'table', 'keyword']);
});

it('offers keywords in the case you are typing them', function () {
    expect(offered(complete('sel')))->toBe(['select'])
        ->and(offered(complete('SEL')))->toBe(['SELECT']);
});

it('offers nothing without a word to complete', function (string $typed) {
    expect(complete($typed))->toBeNull();
})->with([
    'select ',
    'select * from users limit 5',
    "select * from users where name = 'us",
    'select * from users -- us',
]);

it('offers nothing when the word is already whole', function () {
    expect(complete('select * from users where email', after: ' = 1'))->toBeNull();
});

it('quotes a name that needs it', function () {
    expect(offered(complete('select * from ord')))->toBe(['orders', '"Order Items"']);
});

it('replaces only the part of the word before the cursor', function () {
    $completion = complete('select * from us');

    expect($completion->start)->toBe(14)
        ->and($completion->end)->toBe(16);
});

it('moves through the list and wraps', function () {
    $completion = complete('select * from us');

    $completion->move(1);
    expect($completion->selected()['text'])->toBe('user_roles');

    $completion->move(1);
    expect($completion->selected()['text'])->toBe('users');

    $completion->move(-1);
    expect($completion->selected()['text'])->toBe('user_roles');
});
