<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\ConnectionPicker;
use App\Tui\RowFormatter;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\confirm;
use function Laravel\Prompts\error;
use function Laravel\Prompts\info;
use function Laravel\Prompts\password;
use function Laravel\Prompts\pause;
use function Laravel\Prompts\select;
use function Laravel\Prompts\text;

class BrowseCommand extends Command
{
    protected $signature = 'browse';

    protected $description = 'Browse and query your databases';

    public function __construct(
        private ConnectionManager $connections,
        private QueryRunner $runner,
        private RowFormatter $formatter,
    ) {
        parent::__construct();
    }

    public function handle(): int
    {
        while (true) {
            $connection = $this->chooseConnection();

            if ($connection === null) {
                return self::SUCCESS;
            }

            if ($failure = $this->connections->test($connection)) {
                error($failure);
                pause('Press ENTER to continue.');

                continue;
            }

            $this->connections->touch($connection);

            $exit = (new Browser($connection, $this->runner, $this->formatter))->prompt();

            if ($exit !== 'connections') {
                return self::SUCCESS;
            }
        }
    }

    private function chooseConnection(): ?Connection
    {
        $connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();

        $picker = new ConnectionPicker($connections);
        $choice = $picker->prompt();

        // Drop the picker before anything else prompts, so its alt screen is
        // gone and the questions are not drawn over the TUI.
        unset($picker);

        if (is_string($choice) && str_starts_with($choice, 'edit:')) {
            $this->editConnection($connections->firstWhere('id', (int) substr($choice, 5)));

            return $this->chooseConnection();
        }

        return match ($choice) {
            'quit', null => null,
            'new' => $this->createConnection(),
            default => $connections->firstWhere('id', (int) $choice),
        };
    }

    /**
     * Edit one field at a time from a menu, and write nothing until Save.
     * Walking every field in order means you cannot back out of a typo, and
     * gives you no way to leave without answering all of them.
     */
    private function editConnection(?Connection $connection): void
    {
        if ($connection === null) {
            return;
        }

        while (true) {
            $field = select(
                label: "Editing {$connection->name}",
                options: $this->editOptions($connection),
                scroll: 12,
            );

            if ($field === 'cancel') {
                info('Nothing changed.');

                return;
            }

            if ($field === 'save') {
                $this->saveEdited($connection);

                return;
            }

            $this->editField($connection, $field);
        }
    }

    /**
     * @return array<string, string>
     */
    private function editOptions(Connection $connection): array
    {
        $fields = $connection->driver === 'sqlite'
            ? ['name' => 'Name', 'database' => 'Path']
            : [
                'name' => 'Name',
                'host' => 'Host',
                'port' => 'Port',
                'database' => 'Database',
                'username' => 'Username',
                'password' => 'Password',
            ];

        $options = [];

        foreach ($fields as $key => $label) {
            $value = $key === 'password'
                ? ($connection->password === null || $connection->password === '' ? 'not set' : '••••••••')
                : (string) $connection->{$key};

            $options[$key] = $label.'   '.($value === '' ? '—' : $value);
        }

        return $options + [
            'save' => 'Save',
            'cancel' => 'Cancel, keeping it as it was',
        ];
    }

    private function editField(Connection $connection, string $field): void
    {
        if ($field === 'password') {
            $connection->password = password(label: 'Password');

            return;
        }

        $value = text(
            label: ucfirst($field),
            default: (string) $connection->{$field},
            required: in_array($field, ['name', 'host', 'port'], true),
        );

        $connection->{$field} = $field === 'port' ? (int) $value : $value;
    }

    private function saveEdited(Connection $connection): void
    {
        if ($failure = $this->connections->test($connection)) {
            error($failure);

            if (! confirm('Save it anyway?', default: false)) {
                return;
            }
        }

        $connection->save();

        info('Saved.');
    }

    private function createConnection(): ?Connection
    {
        $driver = select(
            label: 'Driver',
            options: array_combine($this->connections->drivers(), $this->connections->drivers()),
        );

        $name = text(label: 'Name', required: true);

        $connection = $driver === 'sqlite'
            ? Connection::create([
                'name' => $name,
                'driver' => $driver,
                'database' => text(label: 'Path to the .sqlite file', required: true),
            ])
            : Connection::create([
                'name' => $name,
                'driver' => $driver,
                'host' => text(label: 'Host', default: '127.0.0.1', required: true),
                'port' => (int) text(
                    label: 'Port',
                    default: (string) ($driver === 'pgsql' ? 5432 : 3306),
                    required: true,
                ),
                'database' => text(label: 'Database', required: true),
                'username' => text(label: 'Username', required: true),
                'password' => password(label: 'Password'),
            ]);

        if ($failure = $this->connections->test($connection)) {
            error($failure);

            if (! confirm('Save it anyway?', default: false)) {
                $connection->delete();

                return null;
            }

            return $connection;
        }

        info('Connected.');

        return $connection;
    }
}
