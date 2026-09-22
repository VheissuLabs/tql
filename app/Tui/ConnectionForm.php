<?php

namespace App\Tui;

use App\Models\Connection;

/**
 * The edit modal's state. Values live here until saved, so cancelling costs
 * nothing and the connection on disk is untouched until you ask for it.
 */
class ConnectionForm
{
    public int $index = 0;

    public bool $editing = false;

    public string $buffer = '';

    /** @var array<string, string> */
    public array $values = [];

    public ?string $error = null;

    public function __construct(public Connection $connection)
    {
        foreach ($this->fields() as $key => $label) {
            $this->values[$key] = (string) ($connection->{$key} ?? '');
        }
    }

    /**
     * @return array<string, string>
     */
    public function fields(): array
    {
        return $this->connection->driver === 'sqlite'
            ? ['name' => 'Name', 'database' => 'Path']
            : [
                'name' => 'Name',
                'host' => 'Host',
                'port' => 'Port',
                'database' => 'Database',
                'username' => 'Username',
                'password' => 'Password',
            ];
    }

    public function keys(): array
    {
        return array_keys($this->fields());
    }

    public function currentKey(): string
    {
        return $this->keys()[$this->index] ?? 'name';
    }

    public function move(int $by): void
    {
        $this->index = max(0, min(count($this->keys()) - 1, $this->index + $by));
    }

    public function start(): void
    {
        $this->editing = true;
        $this->buffer = $this->values[$this->currentKey()] ?? '';
    }

    public function commit(): void
    {
        $this->values[$this->currentKey()] = $this->buffer;
        $this->editing = false;
    }

    public function abandon(): void
    {
        $this->editing = false;
        $this->buffer = '';
    }

    public function type(string $text): void
    {
        $this->buffer .= $text;
    }

    public function backspace(): void
    {
        $this->buffer = mb_substr($this->buffer, 0, -1);
    }

    /**
     * What to show for a field: the live buffer while typing, dots for a
     * password that is not being typed, otherwise the value.
     */
    public function display(string $key): string
    {
        if ($this->editing && $key === $this->currentKey()) {
            return $this->buffer;
        }

        $value = $this->values[$key] ?? '';

        if ($key === 'password') {
            return $value === '' ? '' : str_repeat('•', min(8, mb_strlen($value)));
        }

        return $value;
    }

    public function save(): ?string
    {
        if (trim($this->values['name'] ?? '') === '') {
            return 'A name is required.';
        }

        $taken = Connection::where('name', $this->values['name'])
            ->where('id', '!=', $this->connection->id)
            ->exists();

        if ($taken) {
            return 'Another connection is already called that.';
        }

        $values = $this->values;
        $values['port'] = isset($values['port']) ? (int) $values['port'] : null;

        $this->connection->forceFill(array_filter(
            $values,
            fn ($value) => $value !== null,
        ))->save();

        return null;
    }
}
