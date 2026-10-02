<?php

namespace App\Updates;

use Illuminate\Support\Facades\Http;
use Throwable;

class Updater
{
    public static function asset(): string
    {
        $system = PHP_OS_FAMILY === 'Darwin'
            ? 'macos'
            : 'linux';

        $machine = in_array(php_uname('m'), ['arm64', 'aarch64'], true)
            ? 'aarch64'
            : 'x86_64';

        return "tql-{$system}-{$machine}";
    }

    public function install(string $version, string $target): void
    {
        $expected = $this->checksumFor($version);
        $binary = $this->fetch(Releases::download($version).'/'.self::asset(), 120);

        if ($binary === null) {
            throw new UpdateFailed('could not download '.self::asset()." for v{$version}");
        }

        if (! hash_equals($expected, hash('sha256', $binary))) {
            throw new UpdateFailed("the download for v{$version} does not match its checksum, so it was not installed");
        }

        $this->swapIn($binary, $version, $target);
    }

    private function checksumFor(string $version): string
    {
        $sums = $this->fetch(Releases::download($version).'/SHA256SUMS', 30);

        if ($sums === null) {
            throw new UpdateFailed("v{$version} publishes no checksums, so tql will not install it unchecked");
        }

        if (preg_match('/^([0-9a-f]{64})\s+\*?'.preg_quote(self::asset(), '/').'$/m', $sums, $match) !== 1) {
            throw new UpdateFailed("v{$version} has no checksum for ".self::asset());
        }

        return $match[1];
    }

    private function fetch(string $url, int $seconds): ?string
    {
        try {
            $response = Http::timeout($seconds)->get($url);
        } catch (Throwable) {
            return null;
        }

        return $response->successful()
            ? $response->body()
            : null;
    }

    private function swapIn(string $binary, string $version, string $target): void
    {
        $directory = dirname($target);

        if (! is_writable($directory)) {
            throw new UpdateFailed("{$directory} is not writable; run tql update with sudo");
        }

        $staged = $directory.'/.tql-'.$version.'-'.bin2hex(random_bytes(4));

        try {
            file_put_contents($staged, $binary);
            chmod($staged, 0755);

            exec(escapeshellarg($staged).' --version 2>&1', $output, $status);

            if ($status !== 0 || Version::of(implode("\n", $output)) !== $version) {
                throw new UpdateFailed("the download for v{$version} did not run as v{$version}, so it was not installed");
            }

            if (! rename($staged, $target)) {
                throw new UpdateFailed("could not replace {$target}");
            }
        } finally {
            if (file_exists($staged)) {
                unlink($staged);
            }
        }
    }
}
