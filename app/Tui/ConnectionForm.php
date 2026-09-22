<?php

namespace App\Tui;

use App\Models\Connection;
use App\Ssh\Settings as SshSettings;
use App\Support\KeyFiles;

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
        $this->values[SshSettings::TOGGLE] = SshSettings::used($connection) ? 'yes' : 'no';

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
        'ssl_mode' => 'SSL mode',
        'ssl_ca' => 'SSL CA cert',
        'ssl_cert' => 'SSL cert',
        'ssl_key' => 'SSL key',
        'color' => 'Color',
        'tag' => 'Tag',
        'read_only' => 'Read only',
    ] + SshSettings::FIELDS;

    public const COLORS = ['', 'red', 'yellow', 'green', 'blue', 'magenta', 'cyan'];

    public const YES_NO = ['no', 'yes'];

    /** Fields that hold a path to a file on this machine. */
    public const FILES = [SshSettings::FILE, 'ssl_ca', 'ssl_cert', 'ssl_key'];

    public const TYPE_IT = 'type a path…';

    public ?Picker $picker = null;

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
    /**
     * Fields with a fixed set of answers are cycled rather than typed.
     *
     * @return array<int, string>|null
     */
    /**
     * Which group a field belongs to, so the form reads as sections rather
     * than seventeen rows in a column.
     */
    public function group(string $key): string
    {
        return match (true) {
            in_array($key, ['driver', 'name'], true) => '',
            str_starts_with($key, 'ssl_') => 'tls',
            $key === 'ssl_mode' => 'tls',
            $key === SshSettings::TOGGLE, str_starts_with($key, 'ssh_') => 'ssh',
            in_array($key, ['color', 'tag', 'read_only'], true) => 'labels',
            default => 'where',
        };
    }

    /**
     * True when this field starts a new group, for the divider.
     */
    public function startsGroup(string $key): bool
    {
        $group = $this->group($key);
        $previous = null;

        foreach (array_keys($this->fields()) as $field) {
            if ($field === $key) {
                return $previous !== $group;
            }

            $previous = $this->group($field);
        }

        return false;
    }

    /**
     * Whether what is shown is a real value or a note about what happens when
     * it is left empty, so the two can look different.
     */
    public function isPlaceholder(string $key): bool
    {
        return ($this->values[$key] ?? '') === ''
            && ! in_array($key, [SshSettings::TOGGLE, 'read_only', 'color'], true)
            && $this->display($key) !== '';
    }

    public function overSsh(): bool
    {
        return ($this->values[SshSettings::TOGGLE] ?? 'no') === 'yes';
    }

    public function choices(string $key): ?array
    {
        return match ($key) {
            'driver' => array_values(array_filter(
                self::DRIVERS,
                fn (string $driver) => in_array($driver, \PDO::getAvailableDrivers(), true),
            )) ?: self::DRIVERS,
            'ssl_mode' => Connection::SSL_MODES,
            'color' => self::COLORS,
            'read_only', SshSettings::TOGGLE => self::YES_NO,
            default => null,
        };
    }

    /**
     * The files this machine already has for a field, so a key is chosen
     * rather than remembered.
     *
     * @return array<int, string>
     */
    public function files(string $key): array
    {
        $found = $key === SshSettings::FILE ? SshSettings::keys() : KeyFiles::certificates();

        return array_merge($found, [self::TYPE_IT]);
    }

    public function openFilePicker(): void
    {
        $key = $this->currentKey();

        if (! in_array($key, self::FILES, true)) {
            return;
        }

        $this->picker = new Picker(
            strtoupper(str_replace('_', ' ', $key)),
            $this->files($key),
            (string) ($this->values[$key] ?? ''),
        );
    }

    public function chooseFile(): void
    {
        $chosen = $this->picker?->selected();

        $this->picker = null;

        if ($chosen === null) {
            return;
        }

        if ($chosen === self::TYPE_IT) {
            $this->start();

            return;
        }

        $this->values[$this->currentKey()] = $chosen;
    }

    public function closePicker(): void
    {
        $this->picker = null;
    }

    public function cycleValue(int $by): void
    {
        $key = $this->currentKey();
        $choices = $this->choices($key);

        if ($choices === null) {
            return;
        }

        if ($key === 'driver') {
            $this->cycleDriver($by);

            return;
        }

        $at = array_search((string) ($this->values[$key] ?? ''), $choices, true);
        $at = $at === false ? 0 : $at;

        $this->values[$key] = $choices[($at + $by + count($choices)) % count($choices)];
    }

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
                'ssl_mode' => 'SSL mode',
            ];

        // The detail fields only matter once the thing they belong to is set,
        // so they stay out of the way until then.
        if ($this->driver() !== 'sqlite' && trim($this->values['ssl_mode'] ?? '') !== '') {
            $fields = $this->insertAfter($fields, 'ssl_mode', [
                'ssl_ca' => 'SSL CA cert',
                'ssl_cert' => 'SSL cert',
                'ssl_key' => 'SSL key',
            ]);
        }

        if ($this->driver() !== 'sqlite') {
            $fields[SshSettings::TOGGLE] = 'Over SSH';

            if ($this->overSsh()) {
                $fields += SshSettings::details();
            }
        }

        $fields['color'] = 'Color';
        $fields['tag'] = 'Tag';
        $fields['read_only'] = 'Read only';

        return $this->creating ? ['driver' => 'Driver'] + $fields : $fields;
    }

    /**
     * @param  array<string, string>  $fields
     * @param  array<string, string>  $extra
     * @return array<string, string>
     */
    private function insertAfter(array $fields, string $after, array $extra): array
    {
        $out = [];

        foreach ($fields as $key => $label) {
            $out[$key] = $label;

            if ($key === $after) {
                $out += $extra;
            }
        }

        return $out;
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
        if (($this->values[$key] ?? '') === '' && ($hint = SshSettings::placeholder($key)) !== null) {
            return $hint;
        }

        if ($key === 'ssl_mode' && ($this->values[$key] ?? '') === '') {
            return 'driver default';
        }

        if ($key === 'color' && ($this->values[$key] ?? '') === '') {
            return 'none';
        }

        if ($key === SshSettings::TOGGLE) {
            return $this->overSsh() ? 'yes' : 'no';
        }

        if ($key === 'read_only') {
            return ($this->values[$key] ?? '') === '1' || ($this->values[$key] ?? '') === 'yes'
                ? 'yes'
                : 'no';
        }

        if ($this->editing && $key === $this->currentKey()) {
            return $this->buffer();
        }

        $value = $this->values[$key] ?? '';

        if ($key === 'password' || $key === SshSettings::SECRET) {
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

        // The switch is not a column, and turning it off clears what it hid.
        $over = $this->overSsh();

        unset($values[SshSettings::TOGGLE]);

        if (! $over) {
            $values = array_merge($values, SshSettings::cleared());
        }

        if (isset($values['port'])) {
            $values['port'] = (int) $values['port'];
        }

        if (isset($values['ssh_port'])) {
            $values['ssh_port'] = $values['ssh_port'] === '' ? null : (int) $values['ssh_port'];
        }

        if (isset($values['read_only'])) {
            $values['read_only'] = in_array($values['read_only'], ['yes', '1', 1, true], true);
        }

        $this->connection->forceFill($values)->save();

        return null;
    }
}
