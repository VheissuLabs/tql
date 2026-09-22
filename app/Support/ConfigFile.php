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

    private static function has(string $contents, string $key): bool
    {
        return preg_match('/^\s*'.preg_quote($key, '/').'\s*=/m', $contents) === 1;
    }

    private static function insert(string $contents, array $setting): string
    {
        $block = "\n".ConfigTemplate::block($setting);
        $header = '['.$setting['section'].']';

        $position = strpos($contents, $header);

        if ($position === false) {
            return rtrim($contents)."\n\n".$header."\n".$block;
        }

        $after = strpos($contents, "\n", $position);

        if ($after === false) {
            return $contents."\n".$block;
        }

        return substr($contents, 0, $after + 1).$block.substr($contents, $after + 1);
    }
}
