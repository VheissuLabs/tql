<?php

use App\Support\Paths;
use Devium\Toml\Toml;

function withConfigFile(string $contents, callable $assert): void
{
    Paths::ensureDirectory();

    $file = Paths::configFile();
    $existing = is_readable($file) ? file_get_contents($file) : null;

    file_put_contents($file, $contents);

    try {
        $shipped = require base_path('config/tql.php');
        config(['tql' => $shipped, 'tql.config_error' => null]);

        try {
            $user = Toml::decode((string) file_get_contents($file), true);
            config(['tql' => array_replace_recursive(config('tql'), is_array($user) ? $user : [])]);
        } catch (Throwable $e) {
            config(['tql.config_error' => 'could not be read: '.$e->getMessage()]);
        }

        $assert();
    } finally {
        $existing === null ? @unlink($file) : file_put_contents($file, $existing);
    }
}

it('reads settings from a toml file', function () {
    withConfigFile(<<<'TOML'
    [ui]
    sql_position = "bottom"
    sidebar_width = 30
    TOML, function () {
        expect(config('tql.ui.sql_position'))->toBe('bottom')
            ->and(config('tql.ui.sidebar_width'))->toBe(30);
    });
});

it('keeps shipped defaults for anything not mentioned', function () {
    withConfigFile(<<<'TOML'
    [ui]
    sql_position = "bottom"
    TOML, function () {
        $shipped = require base_path('config/tql.php');

        expect(config('tql.ui.row_style'))->toBe($shipped['ui']['row_style'])
            ->and(config('tql.ui.sidebar_width'))->toBe($shipped['ui']['sidebar_width']);
    });
});

it('supports comments', function () {
    withConfigFile(<<<'TOML'
    # how the app looks
    [ui]

    # where the editor sits
    sql_position = "bottom"
    TOML, function () {
        expect(config('tql.ui.sql_position'))->toBe('bottom')
            ->and(config('tql.config_error'))->toBeNull();
    });
});

it('reports a broken file instead of crashing', function () {
    withConfigFile("[ui\nsql_position = \"bottom\"\n", function () {
        expect(config('tql.config_error'))->toContain('could not be read')
            ->and(config('tql.ui.sql_position'))->toBe('top');
    });
});

it('ignores an empty file', function () {
    withConfigFile('', function () {
        expect(config('tql.config_error'))->toBeNull()
            ->and(config('tql.ui.sql_position'))->toBe('top');
    });
});

it('reads booleans and integers as their own types', function () {
    withConfigFile(<<<'TOML'
    [ui]
    top_margin = 3
    mouse_row_offset = 0
    TOML, function () {
        expect(config('tql.ui.top_margin'))->toBe(3)
            ->and(config('tql.ui.top_margin'))->toBeInt();
    });
});
