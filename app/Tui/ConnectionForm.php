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

    /** The field being typed into, so it has a real cursor. */
    public ?QueryEditor $editor = null;

    /** @var array<string, string> */
    public array $values = [];

    public ?string $error = null;

    public function __construct(public Connection $connection, public bool $creating = false)
    {
        foreach (self::ALL as $key => $label) {
            $this->values[$key] = (string) ($connection->{$key} ?? '');
        }

        $this->values['driver'] = (string) ($connection->driver ?: 'sqlite');

        if ($creating) {
            $this->applyDriverDefaults();
        }
    }

    private const ALL = [
        'driver' => 'Driver',
        'name' => 'Name',
        'database' => 'Database',
        'host' => 'Host',
        'port' => 'Port',
        'username' => 'Username',
        'password' => 'Password',
        'ssh_host' => 'SSH host',
        'ssh_port' => 'SSH port',
        'ssh_user' => 'SSH user',
        'ssh_key' => 'SSH key',
    ];

    public const DRIVERS = ['sqlite', 'mysql', 'pgsql', 'sqlsrv'];

    public function driver(): string
    {
        return $this->values['driver'] ?: 'sqlite';
    }

    /**
     * Changing the driver changes which fields exist, so keep the cursor in
     * range and fill in the defaults for the new one.
     */
    public function cycleDriver(int $by = 1): void
    {
        $drivers = array_values(array_filter(
            self::DRIVERS,
            fn (string $driver) => in_array($driver, \PDO::getAvailableDrivers(), true),
        )) ?: self::DRIVERS;

        $at = array_search($this->driver(), $drivers, true);
        $at = $at === false ? 0 : $at;

        $was = $this->defaultPort();

        $this->values['driver'] = $drivers[($at + $by + count($drivers)) % count($drivers)];

        // Move the port with the driver unless it was typed by hand.
        if (($this->values['port'] ?? '') === (string) $was) {
            $this->values['port'] = '';
        }

        $this->applyDriverDefaults();

        $this->index = min($this->index, count($this->keys()) - 1);
    }

    private function applyDriverDefaults(): void
    {
        if ($this->driver() === 'sqlite') {
            return;
        }

        if (($this->values['host'] ?? '') === '') {
            $this->values['host'] = '127.0.0.1';
        }

        if (($this->values['port'] ?? '') === '') {
            $this->values['port'] = (string) $this->defaultPort();
        }
    }

    private function defaultPort(): int
    {
        return match ($this->driver()) {
            'pgsql' => 5432,
            'sqlsrv' => 1433,
            default => 3306,
        };
    }

    /**
     * @return array<string, string>
     */
    public function fields(): array
    {
        $fields = $this->driver() === 'sqlite'
            ? ['name' => 'Name', 'database' => 'Path']
            : [
                'name' => 'Name',
                'host' => 'Host',
                'port' => 'Port',
                'database' => 'Database',
                'username' => 'Username',
                'password' => 'Password',
                'ssh_host' => 'SSH host',
            ];

        // The rest of the ssh fields only matter once there is a host to
        // tunnel through, so they stay out of the way until then.
        if ($this->driver() !== 'sqlite' && trim($this->values['ssh_host'] ?? '') !== '') {
            $fields['ssh_port'] = 'SSH port';
            $fields['ssh_user'] = 'SSH user';
            $fields['ssh_key'] = 'SSH key';
        }

        return $this->creating ? ['driver' => 'Driver'] + $fields : $fields;
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

        $this->editor = new QueryEditor(multiline: false);
        $this->editor->set($this->values[$this->currentKey()] ?? '');
        $this->editor->toEnd();
    }

    public function commit(): void
    {
        if ($this->editor !== null) {
            $this->values[$this->currentKey()] = $this->editor->buffer();
        }

        $this->editing = false;
        $this->editor = null;
    }

    public function abandon(): void
    {
        $this->editing = false;
        $this->editor = null;
    }

    public function buffer(): string
    {
        return $this->editor?->buffer() ?? '';
    }

    public function cursor(): int
    {
        return $this->editor?->cursorColumn() ?? 0;
    }

    /**
     * What to show for a field: the live buffer while typing, dots for a
     * password that is not being typed, otherwise the value.
     */
    public function display(string $key): string
    {
        if ($key === 'ssh_key' && ($this->values[$key] ?? '') === '') {
            return 'agent or ~/.ssh/config';
        }

        if ($key === 'ssh_port' && ($this->values[$key] ?? '') === '') {
            return '22';
        }

        if ($this->editing && $key === $this->currentKey()) {
            return $this->buffer();
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

        if ($this->driver() === 'sqlite' && trim($this->values['database'] ?? '') === '') {
            return 'A path to the .sqlite file is required.';
        }

        $taken = Connection::where('name', $this->values['name'])
            ->where('id', '!=', $this->connection->id)
            ->exists();

        if ($taken) {
            return 'Another connection is already called that.';
        }

        $values = array_intersect_key(
            $this->values,
            array_flip(array_merge(['driver'], array_keys($this->fields()))),
        );

        if (isset($values['port'])) {
            $values['port'] = (int) $values['port'];
        }

        $this->connection->forceFill($values)->save();

        return null;
    }
}
