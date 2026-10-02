<?php

namespace App\Updates;

use Illuminate\Support\Facades\Http;
use Throwable;

class Releases
{
    public const LATEST = 'https://api.github.com/repos/VheissuLabs/tql/releases/latest';

    public static function download(string $version): string
    {
        return "https://github.com/VheissuLabs/tql/releases/download/v{$version}";
    }

    public static function latest(): ?string
    {
        try {
            $response = Http::timeout(3)->acceptJson()->get(self::LATEST);
        } catch (Throwable) {
            return null;
        }

        if (! $response->successful()) {
            return null;
        }

        return Version::of((string) $response->json('tag_name', ''));
    }
}
