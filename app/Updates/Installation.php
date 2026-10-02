<?php

namespace App\Updates;

class Installation
{
    public const HOMEBREW = 'homebrew';

    public const PACKAGE = 'package';

    public const BY_ITSELF = 'by-itself';

    public const PHP = 'php';

    public function __construct(public string $how, public string $path) {}

    public static function running(): self
    {
        $path = PHP_SAPI === 'micro'
            ? PHP_BINARY
            : (string) ($_SERVER['SCRIPT_FILENAME'] ?? '');

        return self::of(PHP_SAPI, realpath($path) ?: $path);
    }

    public static function of(string $sapi, string $path): self
    {
        return new self(match (true) {
            $sapi !== 'micro' => self::PHP,
            str_contains($path, '/Cellar/') => self::HOMEBREW,
            str_starts_with($path, '/usr/bin/') => self::PACKAGE,
            default => self::BY_ITSELF,
        }, $path);
    }

    public function updatesItself(): bool
    {
        return $this->how === self::BY_ITSELF;
    }

    public function upgrade(): string
    {
        return match ($this->how) {
            self::HOMEBREW => 'brew upgrade tql',
            self::PACKAGE => 'install the new .deb, .rpm or PKGBUILD',
            self::BY_ITSELF => 'tql update',
            default => 'get it from github.com/VheissuLabs/tql/releases',
        };
    }

    public function relaunch(): array
    {
        return PHP_SAPI === 'micro'
            ? [PHP_BINARY]
            : [PHP_BINARY, $this->path];
    }
}
