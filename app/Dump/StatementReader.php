<?php

namespace App\Dump;

use Generator;
use RuntimeException;

class StatementReader
{
    private const QUOTES = ["'", '"', '`'];

    public function __construct(private bool $backslashEscapes) {}

    public function read(string $path): Generator
    {
        $handle = fopen($path, 'r');

        if ($handle === false) {
            throw new RuntimeException("Could not read [{$path}].");
        }

        try {
            yield from $this->statements($this->characters($handle));
        } finally {
            fclose($handle);
        }
    }

    public function split(string $sql): array
    {
        return iterator_to_array($this->statements($this->charactersOf($sql)), false);
    }

    private function statements(Generator $characters): Generator
    {
        $statement = '';
        $quote = null;
        $escaped = false;
        $inComment = false;
        $previous = '';

        foreach ($characters as $character) {
            if ($inComment) {
                $inComment = $character !== "\n";
                $previous = $character;

                continue;
            }

            if ($quote !== null) {
                $statement .= $character;
                $quote = $this->stillQuoted($quote, $character, $escaped);
                $escaped = ! $escaped && $this->backslashEscapes && $character === '\\';
                $previous = $character;

                continue;
            }

            if ($character === '-' && $previous === '-' && str_ends_with($statement, '-')) {
                $statement = substr($statement, 0, -1);
                $inComment = true;
                $previous = '';

                continue;
            }

            if ($character === ';') {
                if (trim($statement) !== '') {
                    yield trim($statement);
                }

                $statement = '';
                $previous = $character;

                continue;
            }

            if (in_array($character, self::QUOTES, true)) {
                $quote = $character;
            }

            $statement .= $character;
            $previous = $character;
        }

        if (trim($statement) !== '') {
            yield trim($statement);
        }
    }

    private function stillQuoted(string $quote, string $character, bool $escaped): ?string
    {
        if ($escaped || $character !== $quote) {
            return $quote;
        }

        return null;
    }

    private function characters($handle): Generator
    {
        while (($chunk = fread($handle, 1 << 16)) !== false && $chunk !== '') {
            yield from $this->charactersOf($chunk);
        }
    }

    private function charactersOf(string $text): Generator
    {
        $length = strlen($text);

        for ($position = 0; $position < $length; $position++) {
            yield $text[$position];
        }
    }
}
