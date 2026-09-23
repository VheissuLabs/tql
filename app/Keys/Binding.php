<?php

namespace App\Keys;

/**
 * One thing tql does, and the keys that ask for it.
 *
 * The description is what help shows; the label, when there is one, is what
 * the hotkey bar shows. Both read the binding rather than repeating the key,
 * so rebinding a key changes what is on screen with it.
 */
class Binding
{
    /**
     * @param  array<int, string>  $keys
     */
    public function __construct(
        public string $action,
        public array $keys,
        public string $description,
        public ?string $label = null,
        public bool $rebindable = true,
    ) {}

    /**
     * How the key reads in help: the first one, spelled for a human.
     */
    public function shown(): string
    {
        return implode(' / ', array_map(Keys::spell(...), $this->keys));
    }
}
