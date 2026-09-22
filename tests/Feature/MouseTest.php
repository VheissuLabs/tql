<?php

use App\Tui\Layout;
use App\Tui\Mouse;

it('parses a button press', function () {
    expect(Mouse::parse("\e[<0;10;4M"))->toBe([
        'button' => 0, 'column' => 10, 'row' => 4, 'pressed' => true,
    ]);
});

it('parses a release as not pressed', function () {
    expect(Mouse::parse("\e[<0;10;4m")['pressed'])->toBeFalse();
});

it('parses wheel buttons', function () {
    expect(Mouse::parse("\e[<64;1;1M")['button'])->toBe(Mouse::WHEEL_UP)
        ->and(Mouse::parse("\e[<65;1;1M")['button'])->toBe(Mouse::WHEEL_DOWN);
});

it('handles coordinates beyond the legacy 223 limit', function () {
    expect(Mouse::parse("\e[<0;240;80M")['column'])->toBe(240);
});

it('ignores anything that is not a mouse sequence', function () {
    expect(Mouse::parse('q'))->toBeNull()
        ->and(Mouse::parse("\e[A"))->toBeNull();
});

it('maps columns to the correct pane', function () {
    [$from, $to] = Layout::sidebarColumns();
    $grid = Layout::gridFirstColumn();

    expect(Layout::inSidebar($from))->toBeTrue()
        ->and(Layout::inSidebar($to))->toBeTrue()
        ->and(Layout::inSidebar($to + 1))->toBeFalse()
        ->and(Layout::inGrid($grid))->toBeTrue()
        ->and(Layout::inGrid($from))->toBeFalse()
        ->and($grid)->toBeGreaterThan($to);
});

it('returns null for rows above the body', function () {
    $first = Layout::firstBodyRow(1);

    expect(Layout::bodyIndex($first - 1, $first))->toBeNull()
        ->and(Layout::bodyIndex($first, $first))->toBe(0)
        ->and(Layout::bodyIndex($first + 3, $first))->toBe(3);
});

it('shifts the body row with the frame padding', function () {
    expect(Layout::firstBodyRow(2) - Layout::firstBodyRow(0))->toBe(2)
        ->and(Layout::firstBodyRow(0))->toBe(Layout::TOP_BORDER_ROWS + 1);
});
