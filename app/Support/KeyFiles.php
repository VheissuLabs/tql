<?php

namespace App\Support;

/**
 * The key and certificate files already on this machine.
 *
 * Typing out ~/.ssh/id_ed25519 from memory is the worst part of setting up a
 * connection, and the answer is almost always sitting in a known place.
 */
class KeyFiles
{
    /** Where certificates tend to live, relative to home. */
    private const CERT_DIRECTORIES = ['.ssh', '.postgresql', '.mysql', 'certs', 'Downloads'];

    private const CERT_EXTENSIONS = ['pem', 'crt', 'cer', 'key', 'ca'];

    /** Files in ~/.ssh that are never a private key. */
    private const NOT_KEYS = ['known_hosts', 'known_hosts.old', 'config', 'authorized_keys', 'environment'];

    /**
     * @return array<int, string>
     */
    public static function sshKeys(): array
    {
        $found = [];

        foreach (static::glob(static::home().'/.ssh/*') as $path) {
            $name = basename($path);

            if (in_array($name, self::NOT_KEYS, true) || str_ends_with($name, '.pub')) {
                continue;
            }

            if (static::looksPrivate($path)) {
                $found[] = $path;
            }
        }

        return static::tidy($found);
    }

    /**
     * @return array<int, string>
     */
    public static function certificates(): array
    {
        $found = [];

        foreach (self::CERT_DIRECTORIES as $directory) {
            foreach (self::CERT_EXTENSIONS as $extension) {
                $found = array_merge($found, static::glob(static::home().'/'.$directory.'/*.'.$extension));
            }
        }

        foreach (self::CERT_EXTENSIONS as $extension) {
            $found = array_merge($found, static::glob(getcwd().'/*.'.$extension));
        }

        return static::tidy($found);
    }

    /**
     * A private key starts with a PEM header. Reading the first line avoids
     * offering sockets, agent files and whatever else is in ~/.ssh.
     */
    private static function looksPrivate(string $path): bool
    {
        if (! is_file($path) || ! is_readable($path)) {
            return false;
        }

        $handle = fopen($path, 'rb');

        if ($handle === false) {
            return false;
        }

        $first = (string) fgets($handle, 64);

        fclose($handle);

        return str_contains($first, 'PRIVATE KEY');
    }

    /**
     * @return array<int, string>
     */
    private static function glob(string $pattern): array
    {
        return array_values(array_filter(glob($pattern) ?: [], 'is_file'));
    }

    /**
     * @param  array<int, string>  $paths
     * @return array<int, string>
     */
    private static function tidy(array $paths): array
    {
        $paths = array_values(array_unique($paths));

        sort($paths);

        return array_map(static::shorten(...), $paths);
    }

    public static function shorten(string $path): string
    {
        $home = static::home();

        return $home !== '' && str_starts_with($path, $home.'/')
            ? '~'.substr($path, strlen($home))
            : $path;
    }

    private static function home(): string
    {
        return (string) (getenv('HOME') ?: '');
    }
}
