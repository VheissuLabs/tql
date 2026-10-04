<?php

namespace App\Tui;

use Laravel\Prompts\Key;

/**
 * A type-to-filter list, in the shape of Laravel Prompts' search prompt but
 * drawn inside our own frame. Prompts' own select and search block the loop
 * and render a frame of their own, which means leaving the TUI to use one.
 */
class Picker
{
    public const UP = [Key::UP, Key::UP_ARROW, Key::CTRL_P];

    public const DOWN = [Key::DOWN, Key::DOWN_ARROW, Key::CTRL_N];

    public int $index = 0;

    public QueryEditor $query;

    /**
     * @param  array<int, string>  $options
     */
    /**
     * @param  array<int, string>  $options
     * @param  array<string, string>  $colors  option => color, for a list
     *                                         where the color is the point
     */
    public function __construct(
        public string $title,
        public array $options,
        public string $chosen = '',
        public array $colors = [],
        public bool $creates = false,
    ) {
        $this->query = new QueryEditor(multiline: false);

        $at = array_search($chosen, $options, true);
        $this->index = $at === false ? 0 : $at;
    }

    /**
     * @return array<int, string>
     */
    public function matches(): array
    {
        $needle = trim($this->query->buffer());

        if ($needle === '') {
            return $this->options;
        }

        $needle = mb_strtolower($needle);

        return array_values(array_filter(
            $this->options,
            fn (string $option) => str_contains(mb_strtolower($option), $needle),
        ));
    }

    public function move(int $by): void
    {
        $count = count($this->matches()) + ($this->newOption() === null ? 0 : 1);

        if ($count === 0) {
            $this->index = 0;

            return;
        }

        // Wrap, so a long list is reachable from either end.
        $this->index = ($this->index + $by + $count) % $count;
    }

    public function type(string $key): void
    {
        $this->query->handle($key);

        // The list changed under the cursor, so start again from the top.
        $this->index = 0;
    }

    public function colorOf(string $option): string
    {
        return $this->colors[$option] ?? '';
    }

    public function selected(): ?string
    {
        return $this->matches()[$this->index] ?? $this->newOption();
    }

    public function newOption(): ?string
    {
        $name = trim($this->query->buffer());

        return $this->creates && $name !== '' && ! in_array($name, $this->options, true) ? $name : null;
    }

    public function creating(): bool
    {
        return $this->newOption() !== null && $this->index === count($this->matches());
    }
}
