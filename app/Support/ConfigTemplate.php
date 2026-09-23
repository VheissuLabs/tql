<?php

namespace App\Support;

/**
 * The config file tql writes on first run, and tops up afterwards.
 *
 * One short line above each setting and no more: the file is a place to change
 * a value, not a place to read an essay. The essay is in docs/configuration.md.
 */
class ConfigTemplate
{
    /**
     * The version this template is at.
     *
     * Written into the file, and read back to decide which settings are new
     * enough to be worth adding: one that shipped before your file was last
     * topped up is one you deleted on purpose.
     */
    public const VERSION = '0.4.0';

    /**
     * @return array<int, array{section: string, key: string, default: string, comment: array<int, string>}>
     */
    public static function settings(): array
    {
        return [
            ['section' => 'ui', 'key' => 'sql_position', 'default' => '"top"', 'since' => '0.3.0', 'comment' => ['Where the SQL editor sits: "top" or "bottom"']],
            ['section' => 'ui', 'key' => 'sql_always', 'default' => 'false', 'since' => '0.3.0', 'comment' => ['Keep it on screen instead of only after pressing s']],
            ['section' => 'ui', 'key' => 'sql_height', 'default' => '0', 'since' => '0.3.0', 'comment' => ['Rows it takes, 0 picks a third of the frame']],
            ['section' => 'ui', 'key' => 'row_style', 'default' => '"marker"', 'since' => '0.3.0', 'comment' => ['The current row: "marker", "dim-others", "bold", "inverse", "underline"']],
            ['section' => 'ui', 'key' => 'top_margin', 'default' => '1', 'since' => '0.3.0', 'comment' => ['Blank rows above the frame']],
            ['section' => 'ui', 'key' => 'sidebar_width', 'default' => '24', 'since' => '0.3.0', 'comment' => ['Width of the table list']],
            ['section' => 'ui', 'key' => 'modal_ring', 'default' => 'true', 'since' => '0.4.0', 'comment' => ['Ring a modal with a border as well as the box itself']],
            ['section' => 'ui', 'key' => 'inspect_related', 'default' => '10', 'since' => '0.3.0', 'comment' => ['Related rows to load into the inspector, 0 turns it off']],
            ['section' => 'ui', 'key' => 'export_path', 'default' => '""', 'since' => '0.3.0', 'comment' => ['Where exports go, empty uses the last folder you saved one in']],
            ['section' => 'ui', 'key' => 'mouse', 'default' => 'true', 'since' => '0.3.0', 'comment' => ['Click, drag and scroll inside tql']],
            ['section' => 'ui', 'key' => 'double_click_ms', 'default' => '400', 'since' => '0.3.0', 'comment' => ['How close two clicks must be to open the editor']],
            ['section' => 'ui', 'key' => 'mouse_row_offset', 'default' => '0', 'since' => '0.3.0', 'comment' => ['Subtract from reported mouse rows, 1 inside a multiplexer']],
            ['section' => 'ui', 'key' => 'mouse_column_offset', 'default' => '0', 'since' => '0.3.0', 'comment' => ['Subtract from reported mouse columns']],

            ['section' => 'theme', 'key' => 'border', 'default' => '"dim"', 'since' => '0.3.0', 'comment' => ['A pane border that is not focused']],
            ['section' => 'theme', 'key' => 'focus_border', 'default' => '"cyan"', 'since' => '0.3.0', 'comment' => ['The border of the pane you are in']],
            ['section' => 'theme', 'key' => 'focus_title', 'default' => '"cyan"', 'since' => '0.3.0', 'comment' => ['Its title']],
            ['section' => 'theme', 'key' => 'grid', 'default' => '"dim"', 'since' => '0.3.0', 'comment' => ['Column separators and the rule under the header, "inherit" follows the pane']],
            ['section' => 'theme', 'key' => 'cursor', 'default' => '"default"', 'since' => '0.3.0', 'comment' => ['The block you are on, "default" swaps the terminal\'s own colors']],
            ['section' => 'theme', 'key' => 'selection', 'default' => '"default"', 'since' => '0.3.0', 'comment' => ['Highlighted but not where you are: the selected table, a marked row']],
            ['section' => 'theme', 'key' => 'edited', 'default' => '"yellow"', 'since' => '0.3.0', 'comment' => ['A row you have changed, before :w']],
            ['section' => 'theme', 'key' => 'deleted', 'default' => '"red"', 'since' => '0.3.0', 'comment' => ['A row marked for deletion, before :w']],
            ['section' => 'theme', 'key' => 'modal_border', 'default' => '"gray"', 'since' => '0.3.0', 'comment' => ['A modal border, and the ring around it']],
            ['section' => 'theme', 'key' => 'modal_focus_border', 'default' => '"cyan"', 'since' => '0.3.0', 'comment' => ['The same, focused']],
            ['section' => 'theme', 'key' => 'modal_title', 'default' => '"white"', 'since' => '0.3.0', 'comment' => ['A modal title']],
            ['section' => 'theme', 'key' => 'modal_focus_title', 'default' => '"cyan"', 'since' => '0.3.0', 'comment' => ['The same, focused']],

            ['section' => 'ai', 'key' => 'provider', 'default' => '"auto"', 'since' => '0.3.0', 'comment' => ['"auto" takes the first provider you have a key for']],
            ['section' => 'ai', 'key' => 'model', 'default' => '""', 'since' => '0.3.0', 'comment' => ['Empty picks a default for that provider']],
            ['section' => 'ai', 'key' => 'timeout', 'default' => '60', 'since' => '0.3.0', 'comment' => ['Seconds to wait for an answer']],
            ['section' => 'ai', 'key' => 'url', 'default' => '""', 'since' => '0.3.0', 'comment' => ['An OpenAI-compatible endpoint instead, such as LM Studio on http://localhost:1234/v1']],
            ['section' => 'ai', 'key' => 'key', 'default' => '""', 'since' => '0.3.0', 'comment' => ['Bearer token for that endpoint, if it wants one']],

            ['section' => 'icons', 'key' => 'mysql', 'default' => '"'."\u{e704}".'"', 'since' => '0.3.0', 'comment' => []],
            ['section' => 'icons', 'key' => 'pgsql', 'default' => '"'."\u{e76e}".'"', 'since' => '0.3.0', 'comment' => []],
            ['section' => 'icons', 'key' => 'sqlite', 'default' => '"'."\u{e7c4}".'"', 'since' => '0.3.0', 'comment' => []],
            ['section' => 'icons', 'key' => 'sqlsrv', 'default' => '"'."\u{f1c0}".'"', 'since' => '0.3.0', 'comment' => []],
            ['section' => 'icons', 'key' => 'default', 'default' => '"'."\u{f1c0}".'"', 'since' => '0.3.0', 'comment' => ['Anything else']],
        ];
    }

    /**
     * A line under the section header, where something is true of all of it.
     *
     * @return array<string, string>
     */
    public static function notes(): array
    {
        return [
            'theme' => 'Colors: dim, default, black, red, green, yellow, blue, magenta, cyan, white, gray',
            'ai' => 'What answers when you press a. Only table and column names are sent, never rows.',
            'icons' => 'The glyph beside a connection name, by driver. Nerd Font devicons.',
        ];
    }

    /**
     * @return array<int, string>
     */
    public static function sections(): array
    {
        return array_values(array_unique(array_column(static::settings(), 'section')));
    }

    public static function render(): string
    {
        $out = '# tql configuration '.static::VERSION."\n"
            ."# Every setting below is its default. Change what you like, delete what you do not.\n"
            ."# All of them, said properly:\n"
            ."# https://github.com/VheissuLabs/tql/blob/main/docs/configuration.md\n";

        $notes = static::notes();

        foreach (static::sections() as $section) {
            $out .= "\n[{$section}]\n";

            if (isset($notes[$section])) {
                $out .= '# '.$notes[$section]."\n";
            }

            foreach (static::settings() as $setting) {
                if ($setting['section'] !== $section) {
                    continue;
                }

                $out .= "\n".static::block($setting);
            }
        }

        return $out;
    }

    /**
     * @param  array{key: string, default: string, comment: array<int, string>}  $setting
     */
    public static function block(array $setting): string
    {
        $out = '';

        foreach ($setting['comment'] as $line) {
            $out .= "# {$line}\n";
        }

        return $out.$setting['key'].' = '.$setting['default']."\n";
    }
}
