<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\RowFormatter;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\confirm;
use function Laravel\Prompts\error;
use function Laravel\Prompts\info;
use function Laravel\Prompts\note;
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

            (new Browser($connection, $this->runner, $this->formatter))->prompt();
        }
    }

    private function chooseConnection(): ?Connection
    {
        $connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();

        if ($connections->isEmpty()) {
            note('No connections yet.');

            return $this->createConnection();
        }

        $options = $connections
            ->mapWithKeys(fn (Connection $c) => [(string) $c->id => $c->name])
            ->all();

        $choice = select(
            label: 'Connections',
            options: $options + ['new' => '+ Add a connection', 'quit' => 'Quit'],
            scroll: 15,
        );

        return match ($choice) {
            'quit' => null,
            'new' => $this->createConnection(),
            default => $connections->firstWhere('id', (int) $choice),
        };
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
