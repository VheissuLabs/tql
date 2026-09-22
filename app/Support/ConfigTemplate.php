<?php

namespace App\Support;

class ConfigTemplate
{
    public static function settings(): array
    {
        return [
            [
                'section' => 'theme',
                'key' => 'border',
                'default' => '"dim"',
                'comment' => [
                    'Colour of a pane border when it is not focused.',
                    'dim, default, black, red, green, yellow, blue, magenta, cyan, white, gray',
                ],
            ],
            [
                'section' => 'theme',
                'key' => 'focus_border',
                'default' => '"cyan"',
                'comment' => ['Colour of the border of the pane you are in'],
            ],
            [
                'section' => 'theme',
                'key' => 'focus_title',
                'default' => '"cyan"',
                'comment' => ['Colour of the title of the pane you are in'],
            ],
            [
                'section' => 'theme',
                'key' => 'grid',
                'default' => '"dim"',
                'comment' => [
                    'Colour of the grid inside a table: the column separators,',
                    'the rule under the header, and the ticks where they meet',
                    'the frame. Set it to "inherit" and the whole table takes',
                    'the pane colour, so a focused table tints all the way',
                    'through instead of just gaining a coloured outline.',
                ],
            ],
            [
                'section' => 'theme',
                'key' => 'cursor',
                'default' => '"default"',
                'comment' => [
                    'Colour of the block you are on: the selected cell, and the',
                    'caret in the SQL and value editors. "default" swaps the',
                    'terminal\'s own colours, which is what it has always done.',
                ],
            ],
            [
                'section' => 'theme',
                'key' => 'selection',
                'default' => '"default"',
                'comment' => [
                    'Colour of what is highlighted but is not where you are: the',
                    'selected table, a highlighted row, and lines picked out with',
                    'V in the value inspector.',
                ],
            ],
            [
                'section' => 'ui',
                'key' => 'sql_position',
                'default' => '"top"',
                'comment' => ['Where the SQL editor sits: "top" or "bottom"'],
            ],
            [
                'section' => 'ui',
                'key' => 'sql_always',
                'default' => 'false',
                'comment' => ['Keep the SQL editor on screen instead of only after pressing s'],
            ],
            [
                'section' => 'ui',
                'key' => 'sql_height',
                'default' => '0',
                'comment' => ['How many rows the SQL editor takes, 0 picks a third of the frame'],
            ],
            [
                'section' => 'ui',
                'key' => 'row_style',
                'default' => '"marker"',
                'comment' => [
                    'How the current row is shown:',
                    '"marker", "dim-others", "bold", "inverse", "underline"',
                ],
            ],
            [
                'section' => 'ui',
                'key' => 'top_margin',
                'default' => '1',
                'comment' => ['Blank rows above the frame'],
            ],
            [
                'section' => 'ui',
                'key' => 'sidebar_width',
                'default' => '24',
                'comment' => ['Width of the tables pane'],
            ],
            [
                'section' => 'ui',
                'key' => 'export_path',
                'default' => '""',
                'comment' => ['Where :export writes files, empty uses ~/.config/tql/exports'],
            ],
            [
                'section' => 'ui',
                'key' => 'mouse_row_offset',
                'default' => '0',
                'comment' => [
                    'Subtract this from reported mouse rows.',
                    'Set to 1 inside a multiplexer whose tab bar sits above the pane.',
                ],
            ],
            [
                'section' => 'ui',
                'key' => 'mouse_column_offset',
                'default' => '0',
                'comment' => ['Subtract this from reported mouse columns'],
            ],
        ];
    }

    public static function sectionFor(string $key): ?string
    {
        foreach (static::settings() as $setting) {
            if ($setting['key'] === $key) {
                return $setting['section'];
            }
        }

        return null;
    }

    public static function sections(): array
    {
        return array_values(array_unique(array_column(static::settings(), 'section')));
    }

    public static function render(): string
    {
        $out = "# tql configuration\n".
            "# Every setting below is the default. Change what you like, delete what you do not.\n";

        foreach (static::sections() as $section) {
            $out .= "\n[{$section}]\n";

            foreach (static::settings() as $setting) {
                if ($setting['section'] !== $section) {
                    continue;
                }

                $out .= "\n".static::block($setting);
            }
        }

        return $out;
    }

    public static function block(array $setting): string
    {
        $out = '';

        foreach ($setting['comment'] as $line) {
            $out .= "# {$line}\n";
        }

        return $out.$setting['key'].' = '.$setting['default']."\n";
    }
}
