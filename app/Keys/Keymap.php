<?php

namespace App\Keys;

use Laravel\Prompts\Key;

/**
 * Every action the grid answers to, and the key that asks for it.
 *
 * One list, read by three things: the key handler, the help modal and the
 * hotkey bar. A key written in [keys] in config.toml replaces the default, and
 * all three follow it, which is the whole reason this is a list rather than a
 * match statement with the keys typed into it.
 */
class Keymap
{
    /** Movement and the keys a terminal names, which are not worth rebinding. */
    public const FIXED = ['move_up', 'move_down', 'move_left', 'move_right', 'activate', 'escape', 'command'];

    /**
     * @var array<string, Binding>|null
     */
    private static ?array $resolved = null;

    /**
     * The defaults, in the order help reads them.
     *
     * @return array<int, Binding>
     */
    public static function defaults(): array
    {
        return [
            new Binding('move_up', [Key::UP, Key::UP_ARROW, 'k'], 'move the cursor up', rebindable: false),
            new Binding('move_down', [Key::DOWN, Key::DOWN_ARROW, 'j'], 'move the cursor down', rebindable: false),
            new Binding('move_left', [Key::LEFT, Key::LEFT_ARROW, 'h'], 'move a column left', rebindable: false),
            new Binding('move_right', [Key::RIGHT, Key::RIGHT_ARROW, 'l'], 'move a column right', rebindable: false),
            new Binding('activate', [Key::ENTER], 'open a table, or edit the value', rebindable: false),
            new Binding('escape', [Key::ESCAPE], 'go back, clear a filter, close what is open', rebindable: false),
            new Binding('command', [':'], 'the command line', rebindable: false),

            new Binding('next_pane', [Key::TAB], 'next pane', 'Pane', rebindable: false),
            new Binding('previous_pane', [Key::SHIFT_TAB], 'previous pane', rebindable: false),

            new Binding('inspect_row', ['i'], 'inspect the row: record and related, foldable', 'Row'),
            new Binding('view_value', ['I'], 'view just this value, read only'),
            new Binding('edit_value', ['e'], 'edit the value', 'Edit'),
            new Binding('edit_row', ['E'], 'edit the whole row in a form'),
            new Binding('sort_column', ['o'], 'sort this column: asc, desc, primary key', 'Sort'),
            new Binding('mark_delete', ['d'], 'mark the row for deletion', 'Mark'),
            new Binding('clear_marks', ['u'], 'drop every pending change'),
            new Binding('new_row', ['N'], 'add a row in a form, written with :w'),
            new Binding('ask', ['a'], 'ask for a query in plain english', 'Ask'),
            new Binding('filter_rows', ['f'], 'filter the rows: column, operator, value', 'Filter'),
            new Binding('filter_tables', ['/'], 'filter the tables list'),
            new Binding('structure', ['t'], 'structure: columns, types, keys and indexes', 'Structure'),
            new Binding('databases', ['b'], 'switch database on this server', 'Database'),
            new Binding('sql', ['s'], 'the SQL editor', 'SQL'),
            new Binding('follow_link', ['L'], 'follow a link: the key under the cursor, or what points here'),
            new Binding('jump_back', ["\x0f"], 'go back where you followed from'),
            new Binding('next_page', ['n'], 'next page'),
            new Binding('previous_page', ['p'], 'previous page'),
            new Binding('reload', ['r'], 'reload the table'),
            new Binding('connections', ['c'], 'back to the connection list'),
            new Binding('yank_value', ['y'], 'yank this value'),
            new Binding('yank_row', ['Y'], 'yank the row as an object'),
            new Binding('narrow', [',', '<'], 'narrow the column'),
            new Binding('widen', ['.', '>'], 'widen the column'),
            new Binding('reset_width', ['='], 'reset the column width'),
            new Binding('help', ['?'], 'help', 'Help'),
            new Binding('quit', ['q'], 'quit'),
            new Binding('redraw', ["\x0c"], 'redraw the screen', rebindable: false),
        ];
    }

    /**
     * The bindings in force: the defaults, with anything [keys] says over them.
     *
     * @return array<string, Binding>
     */
    public static function all(): array
    {
        if (self::$resolved !== null) {
            return self::$resolved;
        }

        $bindings = [];

        foreach (self::defaults() as $binding) {
            $bindings[$binding->action] = $binding;
        }

        foreach ((array) config('tql.keys', []) as $action => $keys) {
            $binding = $bindings[(string) $action] ?? null;

            if ($binding === null || ! $binding->rebindable) {
                continue;
            }

            $bytes = array_values(array_filter(
                array_map(Keys::bytes(...), array_map('strval', (array) $keys)),
            ));

            if ($bytes !== []) {
                $binding->keys = $bytes;
            }
        }

        return self::$resolved = $bindings;
    }

    /**
     * Forget what was read, for a test that changes the config.
     */
    public static function forget(): void
    {
        self::$resolved = null;
    }

    public static function binding(string $action): ?Binding
    {
        return self::all()[$action] ?? null;
    }

    /**
     * What this key asks for, if anything.
     *
     * Later bindings do not steal a key from earlier ones, so a rebind that
     * collides is inert rather than surprising — `tql config` says so.
     */
    public static function action(string $key): ?string
    {
        foreach (self::all() as $action => $binding) {
            if (in_array($key, $binding->keys, true)) {
                return $action;
            }
        }

        return null;
    }

    /**
     * The first key for an action, for a hint or a hotkey.
     */
    public static function key(string $action): string
    {
        return Keys::spell(self::binding($action)->keys[0] ?? '');
    }

    /**
     * Keys asked for by more than one action, which is a config worth
     * mentioning rather than silently half-applying.
     *
     * @return array<int, string>
     */
    public static function clashes(): array
    {
        $seen = [];
        $clashing = [];

        foreach (self::all() as $binding) {
            foreach ($binding->keys as $key) {
                if (isset($seen[$key])) {
                    $clashing[] = Keys::spell($key).' is both '.$seen[$key].' and '.$binding->action;
                }

                $seen[$key] = $binding->action;
            }
        }

        return $clashing;
    }
}
