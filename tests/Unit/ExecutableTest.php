<?php

use App\Support\Executable;

it('finds the standalone binary through the script it runs, since micro leaves PHP_BINARY empty', function () {
    $executable = Executable::of('micro', '', '/usr/local/bin/tql');

    expect($executable->path)->toBe('/usr/local/bin/tql')
        ->and($executable->command())->toBe(['/usr/local/bin/tql']);
});

it('runs a phar or a checkout through the PHP that is running it', function () {
    $executable = Executable::of('cli', '/opt/homebrew/bin/php', '/Users/someone/Code/tql/tql');

    expect($executable->path)->toBe('/Users/someone/Code/tql/tql')
        ->and($executable->command())->toBe(['/opt/homebrew/bin/php', '/Users/someone/Code/tql/tql']);
});
