<?php

namespace App\TablePlus;

use DOMDocument;
use DOMElement;
use RuntimeException;
use Symfony\Component\Process\ExecutableFinder;
use Symfony\Component\Process\Process;

class Plist
{
    public static function read(string $path): mixed
    {
        $document = new DOMDocument;

        if (! @$document->loadXML(static::xml($path), LIBXML_NONET)) {
            throw new RuntimeException("{$path} is not a property list tql can read.");
        }

        $root = $document->getElementsByTagName('plist')->item(0)?->firstElementChild;

        return $root === null
            ? null
            : static::value($root);
    }

    private static function xml(string $path): string
    {
        $contents = (string) file_get_contents($path);

        if (str_starts_with(ltrim($contents), '<?xml') || (new ExecutableFinder)->find('plutil') === null) {
            return $contents;
        }

        $process = new Process(['plutil', '-convert', 'xml1', '-o', '-', $path]);
        $process->run();

        return $process->isSuccessful()
            ? $process->getOutput()
            : $contents;
    }

    private static function value(DOMElement $element): mixed
    {
        return match ($element->tagName) {
            'dict' => static::dictionary($element),
            'array' => array_map(static::value(...), static::children($element)),
            'integer' => (int) $element->textContent,
            'real' => (float) $element->textContent,
            'true' => true,
            'false' => false,
            default => $element->textContent,
        };
    }

    private static function dictionary(DOMElement $element): array
    {
        $dictionary = [];
        $key = null;

        foreach (static::children($element) as $child) {
            if ($child->tagName === 'key') {
                $key = $child->textContent;

                continue;
            }

            if ($key !== null) {
                $dictionary[$key] = static::value($child);
                $key = null;
            }
        }

        return $dictionary;
    }

    private static function children(DOMElement $element): array
    {
        $children = [];

        foreach ($element->childNodes as $node) {
            if ($node instanceof DOMElement) {
                $children[] = $node;
            }
        }

        return $children;
    }
}
