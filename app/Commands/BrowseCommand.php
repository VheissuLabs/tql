<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\RowFormatter;
use App\Tui\Screen;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\clear;
use function Laravel\Prompts\confirm;
use function Laravel\Prompts\error;
use function Laravel\Prompts\info;
use function Laravel\Prompts\note;
use function Laravel\Prompts\password;
use function Laravel\Prompts\pause;
use function Laravel\Prompts\search;
use function Laravel\Prompts\select;
use function Laravel\Prompts\table;
use function Laravel\Prompts\text;
use function Laravel\Prompts\textarea;
use function Laravel\Prompts\warning;

class BrowseCommand extends Command
{
    protected $signature = 'browse';

    protected $description = 'Browse and query your databases';

    private const PAGE = 25;

    private ?Connection $connection = null;

    private ?string $table = null;

    private int $offset = 0;

    public function __construct(
        private ConnectionManager $connections,
        private QueryRunner $runner,
        private RowFormatter $formatter,
    ) {
        parent::__construct();
    }

    public function handle(): int
    {
        $screen = Screen::Connections;

        while ($screen !== Screen::Quit) {
            clear();

            $screen = match ($screen) {
                Screen::Connections => $this->connectionScreen(),
                Screen::NewConnection => $this->newConnectionScreen(),
                Screen::Tables => $this->tableScreen(),
                Screen::Rows => $this->rowScreen(),
                Screen::Sql => $this->sqlScreen(),
                Screen::Quit => Screen::Quit,
            };
        }

        return self::SUCCESS;
    }

    private function connectionScreen(): Screen
    {
        $connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();

        if ($connections->isEmpty()) {
            note('No connections yet. Let\'s add one.');

            return Screen::NewConnection;
        }

        $options = $connections
            ->mapWithKeys(fn (Connection $c) => [(string) $c->id => $c->name])
            ->all();

        $choice = select(
            label: 'Connections',
            options: $options + ['new' => '+ Add a connection', 'quit' => 'Quit'],
            scroll: 15,
            hint: 'Enter to open',
        );

        if ($choice === 'quit') {
            return Screen::Quit;
        }

        if ($choice === 'new') {
            return Screen::NewConnection;
        }

        $this->connection = $connections->firstWhere('id', (int) $choice);

        if ($failure = $this->connections->test($this->connection)) {
            error($failure);
            $this->waitForEnter();

            return Screen::Connections;
        }

        $this->connections->touch($this->connection);

        return Screen::Tables;
    }

    private function newConnectionScreen(): Screen
    {
        $driver = select(
            label: 'Driver',
            options: array_combine($this->connections->drivers(), $this->connections->drivers()),
        );

        $name = text(label: 'Name', required: true);

        if ($driver === 'sqlite') {
            $connection = Connection::create([
                'name' => $name,
                'driver' => $driver,
                'database' => text(label: 'Path to the .sqlite file', required: true),
            ]);
        } else {
            $connection = Connection::create([
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
        }

        if ($failure = $this->connections->test($connection)) {
            error($failure);

            if (! confirm('Save it anyway?', default: false)) {
                $connection->delete();
            }
        } else {
            info('Connected.');
        }

        $this->waitForEnter();

        return Screen::Connections;
    }

    private function tableScreen(): Screen
    {
        $tables = $this->runner->tables($this->connection);

        if ($tables === []) {
            warning('That database has no tables.');
            $this->waitForEnter();

            return Screen::Connections;
        }

        note($this->connection->describe());

        $choice = search(
            label: 'Tables',
            options: fn (string $value) => $this->matching($tables, $value),
            scroll: 15,
            hint: 'Type to filter',
        );

        if ($choice === '__back') {
            return Screen::Connections;
        }

        if ($choice === '__sql') {
            return Screen::Sql;
        }

        $this->table = $choice;
        $this->offset = 0;

        return Screen::Rows;
    }

    private function matching(array $tables, string $value): array
    {
        $matches = $value === ''
            ? $tables
            : array_values(array_filter($tables, fn ($t) => str_contains(strtolower($t), strtolower($value))));

        return array_combine($matches, $matches) + ['__sql' => '> Run SQL', '__back' => '< Connections'];
    }

    private function rowScreen(): Screen
    {
        $result = $this->runner->rows($this->connection, $this->table, self::PAGE, $this->offset);

        if ($result->failed()) {
            error($result->error);
            $this->waitForEnter();

            return Screen::Tables;
        }

        note("{$this->connection->name} · {$this->table}");

        if ($result->count() === 0) {
            warning('No rows.');
        } else {
            table($result->headers(), $this->formatter->rows($result->rows));
        }

        $first = $this->offset + 1;
        $last = $this->offset + $result->count();
        note("rows {$first}-{$last} · {$result->durationMs}ms");

        $actions = ['back' => '< Tables', 'sql' => 'Run SQL'];

        if ($result->count() === self::PAGE) {
            $actions = ['next' => 'Next page'] + $actions;
        }

        if ($this->offset > 0) {
            $actions = ['prev' => 'Previous page'] + $actions;
        }

        return match (select(label: 'Actions', options: $actions)) {
            'next' => $this->page(self::PAGE),
            'prev' => $this->page(-self::PAGE),
            'sql' => Screen::Sql,
            default => Screen::Tables,
        };
    }

    private function page(int $by): Screen
    {
        $this->offset = max(0, $this->offset + $by);

        return Screen::Rows;
    }

    private function sqlScreen(): Screen
    {
        $statement = textarea(label: 'SQL', hint: 'Ctrl+D or Escape when done');

        if (trim($statement) === '') {
            return Screen::Tables;
        }

        $result = $this->runner->run($this->connection, $statement, 'tui');

        clear();

        if ($result->failed()) {
            error($result->error);
        } elseif ($result->count() === 0) {
            warning('No rows returned.');
        } else {
            table($result->headers(), $this->formatter->rows($result->rows));
            note("{$result->count()} rows · {$result->durationMs}ms");
        }

        $this->waitForEnter();

        return Screen::Tables;
    }

    private function waitForEnter(): void
    {
        pause('Press ENTER to continue.');
    }
}
