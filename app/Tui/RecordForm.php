<?php

namespace App\Tui;

use App\Database\ColumnDefault;
use App\Support\Now;

class RecordForm
{
    public const UNTOUCHED = 'untouched';

    public const NULL = 'null';

    public const VALUE = 'value';

    public const AUTO = 'auto';

    public const REQUIRED = 'required';

    public int $index = 0;

    public ?QueryEditor $editor = null;

    public bool $expanded = false;

    public bool $json = false;

    public ?string $error = null;

    public bool $discarding = false;

    public int $width = 76;

    private array $initial;

    public function __construct(
        public string $table,
        private array $fields,
        private array $values,
        public ?string $key = null,
        public mixed $keyValue = null,
        public ?int $insertAt = null,
    ) {
        $this->initial = $values;
    }

    public static function adding(string $table, array $columns, string $driver, array $generated, array $given = []): static
    {
        $fields = [];
        $values = [];

        foreach ($columns as $column) {
            $name = (string) ($column['name'] ?? '');
            [$kind, $default] = ColumnDefault::read($column['default'] ?? null, $driver);
            $auto = in_array($name, $generated, true);

            $required = ! $auto && $kind === ColumnDefault::NONE && ! (bool) ($column['nullable'] ?? true);

            $fields[] = self::field($column, $required ? self::REQUIRED : self::hint($auto, $kind, $default));

            if (array_key_exists($name, $given)) {
                $values[$name] = (string) $given[$name];
            } elseif (! $auto && in_array($kind, [ColumnDefault::VALUE, ColumnDefault::NOW], true)) {
                $values[$name] = $default;
            }
        }

        $form = new static($table, $fields, $values);
        $form->index = $form->firstOpen(array_keys($given));

        return $form;
    }

    public static function pending(string $table, array $columns, string $driver, array $generated, array $values, int $at): static
    {
        $fields = [];

        foreach ($columns as $column) {
            [$kind, $default] = ColumnDefault::read($column['default'] ?? null, $driver);

            $fields[] = self::field($column, self::hint(in_array((string) ($column['name'] ?? ''), $generated, true), $kind, $default));
        }

        return new static($table, $fields, array_map(self::text(...), $values), insertAt: $at);
    }

    public static function editing(string $table, array $columns, array $row, string $key): static
    {
        $fields = [];
        $values = [];

        foreach ($columns as $column) {
            $name = (string) ($column['name'] ?? '');

            $fields[] = self::field($column, '', readOnly: $name === $key);
            $values[$name] = self::text($row[$name] ?? null);
        }

        return new static($table, $fields, $values, $key, $row[$key] ?? null);
    }

    private static function field(array $column, string $hint, bool $readOnly = false): array
    {
        return [
            'name' => (string) ($column['name'] ?? ''),
            'type' => (string) ($column['type_name'] ?? $column['type'] ?? ''),
            'nullable' => (bool) ($column['nullable'] ?? true),
            'hint' => $hint,
            'readOnly' => $readOnly,
        ];
    }

    private static function hint(bool $auto, string $kind, ?string $default): string
    {
        return match (true) {
            $auto => self::AUTO,
            $kind === ColumnDefault::EXPRESSION => 'default '.$default,
            default => '',
        };
    }

    private static function text(mixed $value): ?string
    {
        return match (true) {
            $value === null => null,
            is_bool($value) => $value ? '1' : '0',
            is_scalar($value) => (string) $value,
            default => (string) json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
        };
    }

    private function firstOpen(array $given): int
    {
        foreach ($this->fields as $index => $field) {
            if ($field['hint'] !== self::AUTO && ! in_array($field['name'], $given, true)) {
                return $index;
            }
        }

        return 0;
    }

    public function adds(): bool
    {
        return $this->keyValue === null;
    }

    public function fields(): array
    {
        return $this->fields;
    }

    public function current(): array
    {
        return $this->fields[$this->index];
    }

    public function label(array $field): string
    {
        return $field['name'].' ('.$field['type'].'): ';
    }

    public function state(string $name): string
    {
        return match (true) {
            ! array_key_exists($name, $this->values) => self::UNTOUCHED,
            $this->values[$name] === null => self::NULL,
            default => self::VALUE,
        };
    }

    public function value(string $name): ?string
    {
        return $this->values[$name] ?? null;
    }

    public function changed(string $name): bool
    {
        return array_key_exists($name, $this->values) !== array_key_exists($name, $this->initial)
            || ($this->values[$name] ?? null) !== ($this->initial[$name] ?? null);
    }

    public function dirty(): bool
    {
        foreach ($this->fields as $field) {
            if ($this->changed($field['name'])) {
                return true;
            }
        }

        return false;
    }

    public function move(int $by): void
    {
        $this->index = max(0, min(count($this->fields) - 1, $this->index + $by));
        $this->error = null;
    }

    public function jump(int $index): void
    {
        $this->index = max(0, min(count($this->fields) - 1, $index));
        $this->error = null;
    }

    public function room(array $field): int
    {
        return max(8, $this->width - 4 - mb_strlen($this->label($field)));
    }

    public function start(): bool
    {
        $field = $this->current();

        if ($field['readOnly']) {
            $this->error = $field['name'].' names the row, so it is not edited here';

            return false;
        }

        $text = $this->values[$field['name']] ?? '';

        $this->json = Json::looksLikeJson($text);
        $this->expanded = $this->json || str_contains($text, "\n") || mb_strlen($text) >= $this->room($field);

        $this->editor = new QueryEditor;
        $this->editor->set($this->json ? Json::pretty($text) : $text);

        if ($this->json) {
            $this->editor->toStart();
        }

        $this->error = null;

        return true;
    }

    public function type(string $key): void
    {
        $this->editor?->handle($key);

        if (! $this->expanded && str_contains($this->editor?->buffer() ?? '', "\n")) {
            $this->expanded = true;
        }
    }

    public function now(): void
    {
        $this->editor?->set(Now::for($this->current()['type']));
    }

    public function keep(): bool
    {
        if ($this->editor === null) {
            return true;
        }

        $value = $this->editor->buffer();

        if ($this->json && ! Json::looksLikeJson($value)) {
            $this->error = 'not valid json — fix it, or esc to put it back';

            return false;
        }

        if ($this->json) {
            $value = (string) json_encode(json_decode($value), JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        }

        $this->values[$this->current()['name']] = $value;
        $this->abandon();

        return true;
    }

    public function abandon(): void
    {
        $this->editor = null;
        $this->expanded = false;
        $this->json = false;
        $this->error = null;
    }

    public function setNull(): void
    {
        $field = $this->current();

        if ($field['readOnly']) {
            $this->error = $field['name'].' names the row, so it is not edited here';

            return;
        }

        if (! $field['nullable']) {
            $this->error = $field['name'].' is not null';

            return;
        }

        $this->values[$field['name']] = null;
        $this->error = null;
    }

    public function reset(): void
    {
        $name = $this->current()['name'];

        if (array_key_exists($name, $this->initial) && ! $this->adds()) {
            $this->values[$name] = $this->initial[$name];
        } else {
            unset($this->values[$name]);
        }

        $this->error = null;
    }

    public function values(): array
    {
        $types = array_column($this->fields, 'type', 'name');
        $values = [];

        foreach ($this->values as $name => $value) {
            if (! $this->adds() && ! $this->changed($name)) {
                continue;
            }

            $values[$name] = $value !== null && Now::asked($value) ? Now::for($types[$name] ?? null) : $value;
        }

        return $values;
    }
}
