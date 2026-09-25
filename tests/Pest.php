<?php

putenv('XDG_CONFIG_HOME='.__DIR__.'/.scratch');
putenv('NO_ALT_SCREEN=1');
putenv('NO_MOUSE=1');
putenv('NO_TTY_SETUP=1');

use App\Tui\Browser;
use App\Tui\RowDocument;
use Tests\TestCase;

/*
|--------------------------------------------------------------------------
| Test Case
|--------------------------------------------------------------------------
|
| The closure you provide to your test functions is always bound to a specific PHPUnit test
| case class. By default, that class is "PHPUnit\Framework\TestCase". Of course, you may
| need to change it using the "uses()" function to bind a different classes or traits.
|
*/

uses(TestCase::class)->in('Feature');
uses(TestCase::class)->in('Unit');

/*
|--------------------------------------------------------------------------
| Expectations
|--------------------------------------------------------------------------
|
| When you're writing tests, you often need to check that values meet certain conditions. The
| "expect()" function gives you access to a set of "expectations" methods that you can use
| to assert different things. Of course, you may extend the Expectation API at any time.
|
*/

expect()->extend('toBeOne', function () {
    return $this->toBe(1);
});

/*
|--------------------------------------------------------------------------
| Functions
|--------------------------------------------------------------------------
|
| While Pest is very powerful out-of-the-box, you may have some testing code specific to your
| project that you don't want to repeat in every file. Here you can also expose helpers as
| global functions to help you to reduce the number of lines of code in your test files.
|
*/

function editorShown(Browser $browser): string
{
    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    $island = $browser->valueIsland;

    return implode("\n", $island->content($island->innerWidth(), $island->innerHeight()));
}

function settled(Browser $browser): Browser
{
    while ($browser->idle(true)) {
    }

    return $browser;
}

function expanded(Browser $browser): Browser
{
    $folded = fn () => collect($browser->document->lines())->search(
        fn (array $line) => str_starts_with((string) $line['fold'], RowDocument::RELATED.'.') && $browser->document->isFolded($line['fold']),
    );

    while (($line = $folded()) !== false) {
        $browser->document->toggle($line);
    }

    return settled($browser);
}
