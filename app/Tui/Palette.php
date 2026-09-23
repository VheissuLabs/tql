<?php

namespace App\Tui;

class Palette
{
    public const ACTION = 'action';

    public const COMMAND = 'command';

    public const TABLE = 'table';

    public const DATABASE = 'database';

    public const CONNECTION = 'connection';

    public int $index = 0;

    public QueryEditor $query;

    public function __construct(private array $items)
    {
        $this->query = new QueryEditor(multiline: false);
    }

    public static function item(string $kind, string $label, string $hint, string $target): array
    {
        return ['kind' => $kind, 'label' => $label, 'hint' => $hint, 'target' => $target];
    }

    public function items(): array
    {
        return $this->items;
    }

    public function matches(): array
    {
        $needle = mb_strtolower(trim($this->query->buffer()));

        if ($needle === '') {
            return $this->items;
        }

        $ranked = [];

        foreach ($this->items as $order => $item) {
            $score = static::score(mb_strtolower($item['label']), $needle);

            if ($score !== null) {
                $ranked[] = [$score, $order, $item];
            }
        }

        usort($ranked, fn (array $a, array $b) => [$a[0], $a[1]] <=> [$b[0], $b[1]]);

        return array_column($ranked, 2);
    }

    public static function score(string $label, string $needle): ?int
    {
        return match (true) {
            $label === $needle => 0,
            str_starts_with($label, $needle) => 1,
            preg_match('/(^|[\s_:\/.-])'.preg_quote($needle, '/').'/u', $label) === 1 => 2,
            str_contains($label, $needle) => 3,
            static::inOrder($label, $needle) => 4,
            default => null,
        };
    }

    private static function inOrder(string $label, string $needle): bool
    {
        $at = 0;

        foreach (mb_str_split($needle) as $char) {
            $found = mb_strpos($label, $char, $at);

            if ($found === false) {
                return false;
            }

            $at = $found + 1;
        }

        return true;
    }

    public function move(int $by): void
    {
        $count = count($this->matches());

        $this->index = $count === 0 ? 0 : ($this->index + $by + $count) % $count;
    }

    public function type(string $key): void
    {
        $this->query->handle($key);
        $this->index = 0;
    }

    public function selected(): ?array
    {
        return $this->matches()[$this->index] ?? null;
    }
}
