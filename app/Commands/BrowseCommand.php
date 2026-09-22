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

        $choice = (new ConnectionPicker($connections))->prompt();

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
     * Editing reuses the fields from creating, pre-filled. Leaving the
     * password blank keeps the stored one, so you can fix a typo in a host
     * without having to know the password again.
     */
    private function editConnection(?Connection $connection): void
    {
        if ($connection === null) {
            return;
        }

        $connection->name = text(label: 'Name', default: $connection->name, required: true);

        if ($connection->driver === 'sqlite') {
            $connection->database = text(
                label: 'Path to the .sqlite file',
                default: (string) $connection->database,
                required: true,
            );
        } else {
            $connection->host = text(label: 'Host', default: (string) $connection->host, required: true);
            $connection->port = (int) text(label: 'Port', default: (string) $connection->port, required: true);
            $connection->database = text(label: 'Database', default: (string) $connection->database);
            $connection->username = text(label: 'Username', default: (string) $connection->username);

            $password = password(label: 'Password (blank keeps the saved one)');

            if ($password !== '') {
                $connection->password = $password;
            }
        }

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
