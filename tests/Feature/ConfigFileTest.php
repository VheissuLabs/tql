<?php

use App\Support\ConfigFile;
use App\Support\ConfigTemplate;
use App\Support\ConfigTidy;
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

it('puts the settings back in template order, keeping the values', function () {
    $scrambled = <<<'TOML'
    # tql configuration 0.4.0

    [ai]

    # mine
    key = "secret"
    provider = "auto"

    [ui]
    sidebar_width = 40

    # Where the SQL editor sits: "top" or "bottom"
    sql_position = "bottom"
    TOML;

    $moved = [];
    $tidied = ConfigTidy::apply($scrambled, $moved);

    $order = fn (string $key) => mb_strpos($tidied, $key."\n") ?: mb_strpos($tidied, $key.' ');

    expect($order('sql_position'))->toBeLessThan($order('sidebar_width'))
        ->and($order('[ui]'))->toBeLessThan($order('[ai]'))
        ->and($order('provider'))->toBeLessThan($order('key'))
        // The value and the comment the user wrote travel with the setting.
        ->and($tidied)->toContain('key = "secret"')
        ->and($tidied)->toContain('# mine')
        ->and($moved)->not->toBeEmpty();
});

it('moves a setting written above the first section into it', function () {
    $loose = "# tql configuration 0.4.0\nmouse_row_offset = 1\n\n[ui]\nsidebar_width = 40\n";

    $tidied = ConfigTidy::apply($loose);

    expect(mb_strpos($tidied, 'mouse_row_offset'))->toBeGreaterThan(mb_strpos($tidied, '[ui]'));
});

it('keeps a section it has never heard of', function () {
    $extra = "# tql configuration 0.4.0\n\n[ui]\nsidebar_width = 40\n\n[mine]\nwhatever = true\n";

    $tidied = ConfigTidy::apply($extra);

    expect($tidied)->toContain('[mine]')->toContain('whatever = true');
});

it('only adds settings newer than the file says it is', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, '# tql configuration '.ConfigTemplate::VERSION."\n\n[ui]\nsidebar_width = 40\n");

        // sidebar_width is the only [ui] setting here; the rest shipped at
        // 0.3.0, so they were deleted on purpose and stay deleted.
        expect(ConfigFile::ensure()['added'])->toBe([])
            ->and(file_get_contents($file))->not->toContain('sql_position');
    });
});

it('offers a setting from a newer version once, and not again after it is deleted', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, "# tql configuration 0.5.0\n\n[ui]\nsidebar_width = 40\n");

        expect(ConfigFile::ensure()['added'])->toBe(['sql_editor'])
            ->and(file_get_contents($file))->toContain('# tql configuration '.ConfigTemplate::VERSION);

        file_put_contents($file, preg_replace('/^sql_editor = .*$/m', '', file_get_contents($file)));

        expect(ConfigFile::ensure()['added'])->toBe([]);
    });
});

it('offers everything once to a file with no version in it', function () {
    withoutConfigFile(function (string $file) {
        file_put_contents($file, "[ui]\nsidebar_width = 40\n");

        expect(ConfigFile::ensure()['added'])->toContain('sql_position')
            ->and(file_get_contents($file))->toContain('# tql configuration '.ConfigTemplate::VERSION);
    });
});
