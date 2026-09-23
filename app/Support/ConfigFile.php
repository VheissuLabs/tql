<?php

namespace App\Support;

class ConfigFile
{
    public static function ensure(): array
    {
        Paths::ensureDirectory();

        $file = Paths::configFile();

        if (! file_exists($file)) {
            file_put_contents($file, ConfigTemplate::render());
            chmod($file, 0600);

            return ['created' => true, 'added' => []];
        }

        return ['created' => false, 'added' => static::topUp($file)];
    }

    private static function topUp(string $file): array
    {
        $contents = (string) file_get_contents($file);
        $added = [];

        foreach (ConfigTemplate::settings() as $setting) {
            if (static::has($contents, $setting['key'])) {
                continue;
            }

            $contents = static::insert($contents, $setting);
            $added[] = $setting['key'];
        }

        if ($added !== []) {
            file_put_contents($file, $contents);
        }

        return $added;
    }

    /**
     * A key written above the first [section] header lands at the root of the
     * TOML, where nothing reads it. Settings that predate the sections sit
     * there, so move them into the section they belong to rather than
     * silently ignoring what the user set.
     *
     * @param  array<int, string>  $moved
     */
    public static function hoist(array $user, array &$moved = []): array
    {
        foreach (ConfigTemplate::settings() as $setting) {
            ['key' => $key, 'section' => $section] = $setting;

            if (! array_key_exists($key, $user)) {
                continue;
            }

            // A value under the section header is the one the user means.
            if (! isset($user[$section][$key])) {
                $user[$section][$key] = $user[$key];
                $moved[] = $key;
            }

            unset($user[$key]);
        }

        return $user;
    }

    private static function has(string $contents, string $key): bool
    {
        return preg_match('/^\s*'.preg_quote($key, '/').'\s*=/m', $contents) === 1;
    }

    /**
     * Put a new setting where the template says it goes.
     *
     * Not simply after the section header: that lands every new setting at the
     * top, in the order they were added, and a file that reorders itself over
     * time is a file you cannot find anything in.
     */
    private static function insert(string $contents, array $setting): string
    {
        $lines = explode("\n", $contents);
        $header = '['.$setting['section'].']';

        $headerAt = null;

        foreach ($lines as $index => $line) {
            if (trim($line) === $header) {
                $headerAt = $index;
                break;
            }
        }

        if ($headerAt === null) {
            return rtrim($contents)."\n\n".$header."\n\n".ConfigTemplate::block($setting);
        }

        $at = static::afterPrevious($lines, $setting, $headerAt) ?? static::afterHeader($lines, $headerAt);

        array_splice($lines, $at, 0, array_merge([''], explode("\n", rtrim(ConfigTemplate::block($setting), "\n"))));

        return implode("\n", $lines);
    }

    /**
     * The line after the setting this one follows in the template, if the file
     * has it.
     *
     * @param  array<int, string>  $lines
     */
    private static function afterPrevious(array $lines, array $setting, int $headerAt): ?int
    {
        $before = [];

        foreach (ConfigTemplate::settings() as $candidate) {
            if ($candidate['key'] === $setting['key']) {
                break;
            }

            if ($candidate['section'] === $setting['section']) {
                $before[] = $candidate['key'];
            }
        }

        foreach (array_reverse($before) as $key) {
            foreach ($lines as $index => $line) {
                if ($index > $headerAt && preg_match('/^\s*'.preg_quote($key, '/').'\s*=/', $line) === 1) {
                    return $index + 1;
                }
            }
        }

        return null;
    }

    /**
     * Below the header, and below the note under it if there is one.
     *
     * @param  array<int, string>  $lines
     */
    private static function afterHeader(array $lines, int $headerAt): int
    {
        $at = $headerAt + 1;

        while (isset($lines[$at]) && str_starts_with(trim($lines[$at]), '#')) {
            $at++;
        }

        return $at;
    }
}
