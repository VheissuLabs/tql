<?php

namespace App\Support;

/**
 * Put a config file back into the order the template uses, without taking it
 * away from the person who wrote it.
 *
 * The structure is tql's: which sections exist, and the order the settings
 * come in. The content is theirs: the values, which settings they kept, the
 * comments they wrote, and anything tql has never heard of. All of that is
 * carried across, and a comment travels with whatever it is written above.
 */
class ConfigTidy
{
    /**
     * @param  array<int, string>  $moved  the settings that changed places
     */
    public static function apply(string $contents, array &$moved = []): string
    {
        $file = static::parse($contents);
        $order = static::order();

        $out = rtrim(implode("\n", $file['preamble']))."\n";

        foreach ($file['root'] as $entry) {
            $out .= "\n".$entry['text'];
        }

        foreach (static::sections($file) as $section) {
            $entries = $file['sections'][$section] ?? [];

            $known = [];
            $unknown = [];

            foreach ($entries as $entry) {
                $at = array_search($section.'.'.$entry['key'], $order, true);

                $at === false ? $unknown[] = $entry : $known[$at] = $entry;
            }

            ksort($known);

            $sorted = array_merge(array_values($known), $unknown);

            foreach ($entries as $index => $entry) {
                if (($sorted[$index]['key'] ?? null) !== $entry['key']) {
                    $moved[] = $entry['key'];
                }
            }

            $out .= "\n[{$section}]\n";

            foreach ($file['notes'][$section] ?? [] as $note) {
                $out .= $note."\n";
            }

            foreach ($sorted as $entry) {
                $out .= "\n".$entry['text'];
            }
        }

        return $out;
    }

    /**
     * Sections in template order, then any the user added, so a section tql
     * knows nothing about survives being tidied.
     *
     * @param  array{sections: array<string, mixed>}  $file
     * @return array<int, string>
     */
    private static function sections(array $file): array
    {
        $template = ConfigTemplate::sections();
        $theirs = array_keys($file['sections']);

        return array_values(array_unique(array_merge(
            array_values(array_intersect($template, $theirs)),
            array_values(array_diff($theirs, $template)),
        )));
    }

    /**
     * Every known setting as "section.key", in the order they should appear.
     *
     * @return array<int, string>
     */
    private static function order(): array
    {
        return array_map(
            fn (array $setting) => $setting['section'].'.'.$setting['key'],
            ConfigTemplate::settings(),
        );
    }

    /**
     * The section a known setting belongs under.
     */
    private static function sectionFor(string $key): ?string
    {
        foreach (ConfigTemplate::settings() as $setting) {
            if ($setting['key'] === $key) {
                return $setting['section'];
            }
        }

        return null;
    }

    /**
     * Break the file up.
     *
     * A comment block written straight above a setting belongs to it and moves
     * with it. One with a blank line under it is standing on its own — the
     * file's heading, or a note under a section header — and stays where it is.
     *
     * @return array{preamble: array<int, string>, notes: array<string, array<int, string>>, sections: array<string, array<int, array{key: string, text: string}>>, root: array<int, array{key: string, text: string}>}
     */
    private static function parse(string $contents): array
    {
        $preamble = [];
        $notes = [];
        $sections = [];
        $root = [];

        $section = null;
        $pending = [];

        foreach (explode("\n", $contents) as $line) {
            $trimmed = trim($line);

            if (preg_match('/^\[([^\]]+)\]$/', $trimmed, $match) === 1) {
                $section = $match[1];
                $sections[$section] ??= [];
                $pending = [];

                continue;
            }

            if ($trimmed === '') {
                if ($pending !== []) {
                    $section === null
                        ? $preamble = array_merge($preamble, $pending)
                        : $notes[$section] = array_merge($notes[$section] ?? [], $pending);

                    $pending = [];
                }

                continue;
            }

            if (str_starts_with($trimmed, '#')) {
                // The file's own heading belongs to the file, not to whatever
                // happens to be written under it.
                if ($section === null && preg_match('/^#\s*tql configuration/', $trimmed) === 1) {
                    $preamble[] = $line;

                    continue;
                }

                $pending[] = $line;

                continue;
            }

            if (preg_match('/^\s*([A-Za-z0-9_.-]+)\s*=/', $line, $match) !== 1) {
                continue;
            }

            $entry = [
                'key' => $match[1],
                'text' => implode("\n", array_merge($pending, [rtrim($line)]))."\n",
            ];

            $pending = [];

            // A setting written above the first header belongs to no section
            // and is never read. Tidying is the moment to put it where it was
            // meant to go, rather than leaving it somewhere inert.
            $belongs = $section ?? static::sectionFor($entry['key']);

            if ($belongs === null) {
                $root[] = $entry;

                continue;
            }

            $sections[$belongs] ??= [];
            $sections[$belongs][] = $entry;
        }

        // Comments at the very end, with nothing under them.
        if ($pending !== []) {
            $section === null
                ? $preamble = array_merge($preamble, $pending)
                : $notes[$section] = array_merge($notes[$section] ?? [], $pending);
        }

        return ['preamble' => $preamble, 'notes' => $notes, 'sections' => $sections, 'root' => $root];
    }
}
