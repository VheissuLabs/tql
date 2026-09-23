<?php

namespace App\Keys;

use Laravel\Prompts\Key;

/**
 * Turning a key between the three shapes it takes: what a terminal sends,
 * what a person types into a config file, and what is printed on screen.
 */
class Keys
{
    /**
     * Names somebody might reasonably write in config.toml, and the bytes
     * they stand for.
     *
     * @var array<string, string>
     */
    private const NAMED = [
        'tab' => Key::TAB,
        'shift+tab' => Key::SHIFT_TAB,
        'enter' => Key::ENTER,
        'escape' => Key::ESCAPE,
        'esc' => Key::ESCAPE,
        'space' => ' ',
        'backspace' => Key::BACKSPACE,
        'delete' => Key::DELETE,
        'up' => Key::UP,
        'down' => Key::DOWN,
        'left' => Key::LEFT,
        'right' => Key::RIGHT,
        'home' => Key::HOME[0],
        'end' => Key::END[0],
    ];

    /**
     * The bytes a name stands for: "ctrl+o" is one byte, "f" is itself.
     */
    public static function bytes(string $name): ?string
    {
        $name = trim($name);

        if ($name === '') {
            return null;
        }

        if (isset(self::NAMED[strtolower($name)])) {
            return self::NAMED[strtolower($name)];
        }

        if (preg_match('/^ctrl\+([a-z])$/i', $name, $match) === 1) {
            return chr(ord(strtolower($match[1])) - 96);
        }

        if (preg_match('/^(?:alt|option|meta)\+(.)$/iu', $name, $match) === 1) {
            return "\e".$match[1];
        }

        // A single character is itself, and case matters: N is not n.
        return mb_strlen($name) === 1 ? $name : null;
    }

    public static function glyph(string $key): string
    {
        return match ($key) {
            Key::UP, Key::UP_ARROW => '↑',
            Key::DOWN, Key::DOWN_ARROW => '↓',
            Key::LEFT, Key::LEFT_ARROW => '←',
            Key::RIGHT, Key::RIGHT_ARROW => '→',
            Key::ENTER => '↵',
            Key::SHIFT_TAB => '⇧tab',
            Key::BACKSPACE => '⌫',
            default => self::isAlt($key) ? (PHP_OS_FAMILY === 'Darwin' ? '⌥' : 'alt+').substr($key, 1) : self::spell($key),
        };
    }

    /**
     * How a key is written, for help and for a config file.
     */
    public static function spell(string $key): string
    {
        foreach (self::NAMED as $name => $bytes) {
            if ($bytes === $key && $name !== 'esc') {
                return $name;
            }
        }

        if (strlen($key) === 1 && ord($key) > 0 && ord($key) < 27) {
            return 'ctrl+'.chr(ord($key) + 96);
        }

        if (self::isAlt($key)) {
            return 'alt+'.substr($key, 1);
        }

        return $key;
    }

    public static function isAlt(string $key): bool
    {
        return strlen($key) === 2 && $key[0] === "\e" && ctype_graph($key[1]);
    }
}
