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
                'section' => 'theme',
                'key' => 'deleted',
                'default' => '"red"',
                'comment' => ['Colour of a row marked for deletion, before :w writes it'],
            ],
            [
                'section' => 'theme',
                'key' => 'modal_border',
                'default' => '"gray"',
                'comment' => ['Colours for modals: the inspector, help, filters'],
            ],
            [
                'section' => 'theme',
                'key' => 'modal_focus_border',
                'default' => '"cyan"',
                'comment' => [],
            ],
            [
                'section' => 'theme',
                'key' => 'modal_title',
                'default' => '"white"',
                'comment' => [],
            ],
            [
                'section' => 'theme',
                'key' => 'modal_focus_title',
                'default' => '"cyan"',
                'comment' => [],
            ],
            [
                'section' => 'theme',
                'key' => 'edited',
                'default' => '"yellow"',
                'comment' => ['Colour of a row you have edited, before :w writes it'],
            ],
            [
                'section' => 'ai',
                'key' => 'provider',
                'default' => '"auto"',
                'comment' => [
                    'Which provider answers when you press "a". "auto" uses',
                    'whichever api key you have set, such as ANTHROPIC_API_KEY',
                    'or OPENAI_API_KEY. Only table and column names are sent.',
                ],
            ],
            [
                'section' => 'ai',
                'key' => 'model',
                'default' => '""',
                'comment' => ['Empty uses a sensible default for the provider'],
            ],
            [
                'section' => 'ai',
                'key' => 'timeout',
                'default' => '60',
                'comment' => ['Seconds to wait for an answer'],
            ],
            [
                'section' => 'ai',
                'key' => 'url',
                'default' => '""',
                'comment' => [
                    'An OpenAI-compatible endpoint to use instead of a hosted',
                    'provider, such as LM Studio on http://localhost:1234/v1.',
                    'Set "model" to the model you loaded there.',
                ],
            ],
            [
                'section' => 'ai',
                'key' => 'key',
                'default' => '""',
                'comment' => ['Bearer token for that endpoint, if it wants one'],
            ],
            [
                'section' => 'icons',
                'key' => 'mysql',
                'default' => '""',
                'comment' => [
                    'Glyph shown beside a connection name, by driver.',
                    'These are Nerd Font devicons. Change them if you like.',
                ],
            ],
            [
                'section' => 'icons',
                'key' => 'pgsql',
                'default' => '""',
                'comment' => [],
            ],
            [
                'section' => 'icons',
                'key' => 'sqlite',
                'default' => '""',
                'comment' => [],
            ],
            [
                'section' => 'icons',
                'key' => 'sqlsrv',
                'default' => '""',
                'comment' => [],
            ],
            [
                'section' => 'icons',
                'key' => 'default',
                'default' => '""',
                'comment' => ['Anything else'],
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
                'key' => 'inspect_related',
                'default' => '10',
                'comment' => [
                    'Related rows to load into the row inspector, following',
                    'foreign keys both ways. 0 turns it off.',
                ],
            ],
            [
                'section' => 'ui',
                'key' => 'mouse',
                'default' => 'true',
                'comment' => [
                    'Click, drag and scroll inside tql. Turn it off to give the',
                    'terminal its own selection and scrollback back, for copying',
                    'text with the mouse instead of clicking cells with it.',
                ],
            ],
            [
                'section' => 'ui',
                'key' => 'double_click_ms',
                'default' => '400',
                'comment' => ['How close two clicks must be to open the editor'],
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
