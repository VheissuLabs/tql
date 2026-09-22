<?php

namespace App\Tui;

/**
 * A type-to-filter list, in the shape of Laravel Prompts' search prompt but
 * drawn inside our own frame. Prompts' own select and search block the loop
 * and render a frame of their own, which means leaving the TUI to use one.
 */
class Picker
{
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
        $count = count($this->matches());

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
        return $this->matches()[$this->index] ?? null;
    }
}
