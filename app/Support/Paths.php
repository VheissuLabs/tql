<?php

namespace App\Support;

class Paths
{
    public static function configDirectory(): string
    {
        $base = getenv('XDG_CONFIG_HOME') ?: getenv('HOME').'/.config';

        return $base.'/dotsql';
    }

    public static function database(): string
    {
        return static::configDirectory().'/dotsql.sqlite';
    }

    public static function configFile(): string
    {
        return static::configDirectory().'/config.php';
    }

    public static function keyFile(): string
    {
        return static::configDirectory().'/key';
    }

    public static function ensureDirectory(): string
    {
        $directory = static::configDirectory();

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
