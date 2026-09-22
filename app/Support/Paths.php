<?php

namespace App\Support;

class Paths
{
    public static function configDirectory(): string
    {
        $base = getenv('XDG_CONFIG_HOME') ?: getenv('HOME').'/.config';

        return $base.'/tql';
    }

    /**
     * Home-relative paths, since that is how people type them.
     */
    public static function expand(string $path): string
    {
        return str_starts_with($path, '~/')
            ? (string) getenv('HOME').substr($path, 1)
            : $path;
    }

    public static function database(): string
    {
        return static::configDirectory().'/tql.sqlite';
    }

    public static function configFile(): string
    {
        return static::configDirectory().'/config.toml';
    }

    public static function legacyConfigFile(): string
    {
        return static::configDirectory().'/config.php';
    }

    public static function keyFile(): string
    {
        return static::configDirectory().'/key';
    }

    public static function legacyDirectory(): string
    {
        $base = getenv('XDG_CONFIG_HOME') ?: getenv('HOME').'/.config';

        return $base.'/dotsql';
    }

    /**
     * Carry a pre-rename config directory over wholesale. The connection
     * passwords are encrypted with the key file that sits beside them, so the
     * store and the key have to move together or the passwords are lost.
     */
    public static function migrateLegacy(): bool
    {
        $from = static::legacyDirectory();
        $to = static::configDirectory();

        if ($from === $to || is_dir($to) || ! is_dir($from)) {
            return false;
        }

        if (! rename($from, $to)) {
            return false;
        }

        $database = $to.'/dotsql.sqlite';

        if (is_file($database) && ! is_file(static::database())) {
            rename($database, static::database());
        }

        return true;
    }

    public static function ensureDirectory(): string
    {
        $directory = static::configDirectory();

        static::migrateLegacy();

        if (! is_dir($directory)) {
            mkdir($directory, 0700, true);
        }

        return $directory;
    }

    public static function ensureDatabase(): string
    {
        static::ensureDirectory();

        $database = static::database();

        if (! file_exists($database)) {
            touch($database);
            chmod($database, 0600);
        }

        return $database;
    }

    public static function ensureKey(): string
    {
        static::ensureDirectory();

        $file = static::keyFile();

        if (! file_exists($file)) {
            file_put_contents($file, 'base64:'.base64_encode(random_bytes(32)));
            chmod($file, 0600);
        }

        return trim(file_get_contents($file));
    }
}
