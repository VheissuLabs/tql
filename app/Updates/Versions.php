<?php

namespace App\Updates;

use App\Support\Executable;
use Closure;

class Versions
{
    public const DIRECTORY = '.tql-versions';

    public const NEXT = 'next';

    public const KEPT = 3;

    public function __construct(public string $link) {}

    public static function forInstalled(string $path): self
    {
        return basename(dirname($path)) === self::DIRECTORY
            ? new self(dirname($path, 2).'/tql')
            : new self($path);
    }

    public static function activateOnStart(): void
    {
        $executable = Executable::running();

        if (! $executable->standalone() || ! Installation::of($executable->sapi, $executable->path)->updatesItself()) {
            return;
        }

        $versions = self::forInstalled($executable->path);

        if (! $versions->activate()) {
            return;
        }

        pcntl_exec($versions->link, array_slice($_SERVER['argv'], 1), getenv());
    }

    public function directory(): string
    {
        return dirname($this->link).'/'.self::DIRECTORY;
    }

    public function file(string $version): string
    {
        return $this->directory().'/tql-'.$version;
    }

    public function stage(string $version, string $binary): void
    {
        if (! is_writable(dirname($this->link))) {
            throw new UpdateFailed(dirname($this->link).' is not writable; run tql update with sudo');
        }

        if (! is_dir($this->directory())) {
            mkdir($this->directory(), 0755, true);
        }

        $staged = $this->directory().'/.download-'.bin2hex(random_bytes(4));

        try {
            file_put_contents($staged, $binary);
            chmod($staged, 0755);

            exec('SHELL_VERBOSITY=0 '.escapeshellarg($staged).' --version 2>/dev/null', $output, $status);

            if ($status !== 0 || self::reportedVersion($output) !== $version) {
                throw new UpdateFailed("the download for v{$version} did not run as v{$version}, so it was not installed");
            }

            rename($staged, $this->file($version));
        } finally {
            if (file_exists($staged)) {
                unlink($staged);
            }
        }

        file_put_contents($this->directory().'/'.self::NEXT, basename($this->file($version)));

        $this->prune();
    }

    public function switchesSafelyNow(): bool
    {
        return is_link($this->link);
    }

    public function activate(?Closure $othersRunning = null): bool
    {
        $marker = $this->directory().'/'.self::NEXT;

        if (! is_file($marker)) {
            return false;
        }

        $name = trim((string) file_get_contents($marker));

        if (! is_file($this->directory().'/'.$name)) {
            unlink($marker);

            return false;
        }

        $othersRunning ??= self::othersRunning(...);

        if (! is_link($this->link) && file_exists($this->link) && $othersRunning()) {
            return false;
        }

        $temporary = dirname($this->link).'/.tql-link-'.bin2hex(random_bytes(4));

        if (! @symlink(self::DIRECTORY.'/'.$name, $temporary)) {
            return false;
        }

        if (! @rename($temporary, $this->link)) {
            @unlink($temporary);

            return false;
        }

        unlink($marker);

        return true;
    }

    public static function reportedVersion(array $output): ?string
    {
        foreach ($output as $line) {
            if (preg_match('/^Tql v?(\d+\.\d+\.\d+)/', trim($line), $match) === 1) {
                return $match[1];
            }
        }

        return null;
    }

    private function prune(): void
    {
        $files = glob($this->directory().'/tql-*') ?: [];

        usort($files, fn (string $first, string $second) => version_compare(
            (string) Version::of(basename($second)),
            (string) Version::of(basename($first)),
        ));

        foreach (array_slice($files, self::KEPT) as $old) {
            unlink($old);
        }
    }

    private static function othersRunning(): bool
    {
        exec('pgrep -x tql 2>/dev/null', $pids);

        return array_diff(array_map('intval', $pids), [getmypid()]) !== [];
    }
}
