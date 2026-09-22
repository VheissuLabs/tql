<?php

use App\Support\ConfigFile;
use App\Support\ConfigTemplate;
use App\Support\Paths;
use Devium\Toml\Toml;

function withoutConfigFile(callable $test): void
{
    $file = Paths::configFile();
    $existing = is_readable($file) ? file_get_contents($file) : null;

    @unlink($file);

    try {
        $test($file);
    } finally {
        $existing === null ? @unlink($file) : file_put_contents($file, $existing);
    }
}

it('writes a commented config file on first run', function () {
    withoutConfigFile(function (string $file) {
        $result = ConfigFile::ensure();

        expect($result['created'])->toBeTrue()
            ->and(file_exists($file))->toBeTrue();

        $contents = file_get_contents($file);

        foreach (ConfigTemplate::settings() as $setting) {
            expect($contents)->toContain($setting['key'].' = ');

            foreach ($setting['comment'] as $line) {
                expect($contents)->toContain($line);
            }
        }
    });
});

it('writes defaults that parse as valid toml', function () {
    withoutConfigFile(function (string $file) {
        ConfigFile::ensure();

        $decoded = Toml::decode((string) file_get_contents($file), true);

        expect($decoded)->toHaveKey('ui')
            ->and($decoded['ui'])->toHaveKey('sql_position');
    });
});

it('does nothing when every setting is already there', function () {
    withoutConfigFile(function () {
        ConfigFile::ensure();

        $second = ConfigFile::ensure();

        expect($second['created'])->toBeFalse()
            ->and($second['added'])->toBe([]);
    });
});

it('adds only the settings that are missing', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, <<<'TOML'
        [ui]
        sql_position = "bottom"
        TOML);

        $result = ConfigFile::ensure();

        expect($result['created'])->toBeFalse()
            ->and($result['added'])->toContain('sql_always')
            ->and($result['added'])->not->toContain('sql_position');
    });
});

it('never changes a value the user already set', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, <<<'TOML'
        [ui]
        sql_position = "bottom"
        top_margin = 7
        TOML);

        ConfigFile::ensure();

        $decoded = Toml::decode((string) file_get_contents($file), true);

        expect($decoded['ui']['sql_position'])->toBe('bottom')
            ->and($decoded['ui']['top_margin'])->toBe(7);
    });
});

it('keeps the file valid toml after topping up', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, "[ui]\nsql_position = \"bottom\"\n");

        ConfigFile::ensure();

        $decoded = Toml::decode((string) file_get_contents($file), true);

        expect($decoded['ui'])->toHaveKeys(['sql_position', 'sql_always', 'row_style', 'sidebar_width']);
    });
});

it('keeps the comments the user wrote', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, "# my own note\n[ui]\nsql_position = \"bottom\"\n");

        ConfigFile::ensure();

        expect(file_get_contents($file))->toContain('# my own note');
    });
});

it('creates a section that is missing entirely', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, "# empty config\n");

        ConfigFile::ensure();

        $decoded = Toml::decode((string) file_get_contents($file), true);

        expect($decoded)->toHaveKey('ui');
    });
});

it('covers every shipped default', function () {
    $shipped = require base_path('config/tql.php');
    $template = array_column(ConfigTemplate::settings(), 'key');

    foreach (['ui', 'theme'] as $section) {
        foreach (array_keys($shipped[$section]) as $key) {
            expect(in_array($key, $template, true))->toBeTrue("[{$section}.{$key}] is missing from the config template");
        }
    }
});

it('reads settings written above the first section header', function () {
    $hoisted = ConfigFile::hoist([
        'mouse_row_offset' => 1,
        'sidebar_width' => 40,
        'ui' => ['sql_position' => 'bottom'],
    ]);

    expect($hoisted)->toBe([
        'ui' => [
            'sql_position' => 'bottom',
            'sidebar_width' => 40,
            'mouse_row_offset' => 1,
        ],
    ]);
});

it('prefers the sectioned value when a key appears in both places', function () {
    $moved = [];

    $hoisted = ConfigFile::hoist([
        'mouse_row_offset' => 1,
        'ui' => ['mouse_row_offset' => 3],
    ], $moved);

    expect($hoisted['ui']['mouse_row_offset'])->toBe(3)
        ->and($hoisted)->not->toHaveKey('mouse_row_offset')
        ->and($moved)->toBe([]);
});

it('leaves a key it does not recognise where it is', function () {
    expect(ConfigFile::hoist(['something_else' => 1]))->toBe(['something_else' => 1]);
});
