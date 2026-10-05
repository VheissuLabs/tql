<?php

use App\Database\ConnectionManager;
use App\Database\QueryResult;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\ConnectionForm;
use App\Tui\Picker;
use App\Tui\RowFormatter;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    config(['tql.ui.mouse_row_offset' => 0, 'tql.ui.sql_always' => false]);
});

function onSqlite(): Browser
{
    $path = sys_get_temp_dir().'/tql-db-'.uniqid().'.sqlite';
    touch($path);

    (new PDO('sqlite:'.$path))->exec('create table widgets (id integer primary key)');

    $connection = Connection::create([
        'name' => 'db'.uniqid(), 'driver' => 'sqlite', 'database' => $path,
    ]);

    $browser = new Browser($connection, app(QueryRunner::class), app(RowFormatter::class));

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);
    $render->invoke($browser);

    return $browser;
}

it('keeps a session database off the saved record', function () {
    $connection = Connection::create([
        'name' => 'server'.uniqid(), 'driver' => 'mysql', 'host' => 'h',
        'port' => 3306, 'database' => 'shop', 'username' => 'alice',
    ]);

    expect($connection->activeDatabase())->toBe('shop');

    $connection->sessionDatabase = 'reporting';

    expect($connection->activeDatabase())->toBe('reporting')
        ->and($connection->toLaravelConfig()['database'])->toBe('reporting');

    // Saving the record — which happens on every open — must not persist it.
    $connection->forceFill(['last_used_at' => now()])->save();

    expect($connection->fresh()->database)->toBe('shop');
});

it('has no databases to list for sqlite', function () {
    $browser = onSqlite();

    expect(app(QueryRunner::class)->databases($browser->connection))->toBe([]);

    $browser->emit('key', 'b');

    expect($browser->databasePicker)->toBeNull()
        ->and($browser->status)->toContain('one file');
});

it('does not offer the database key for sqlite', function () {
    $browser = onSqlite();

    $render = new ReflectionMethod($browser, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($browser)))
        ->not->toContain('b Database');
});

it('says the database is asked for when the field is left empty', function () {
    $connection = Connection::create([
        'name' => 'empty'.uniqid(), 'driver' => 'mysql', 'host' => 'h', 'port' => 3306,
    ]);

    $form = new ConnectionForm($connection);

    expect($form->display('database'))->toBe('ask on connect');
});

it('leaves a sqlite path alone', function () {
    $connection = Connection::create([
        'name' => 'lite'.uniqid(), 'driver' => 'sqlite', 'database' => '',
    ]);

    $form = new ConnectionForm($connection);

    expect($form->display('database'))->toBe('');
});

it('returns no databases when the server cannot be reached', function () {
    $connection = new Connection([
        'name' => 'unreachable', 'driver' => 'mysql',
        'host' => '127.0.0.1', 'port' => 1, 'database' => 'shop',
    ]);

    expect(app(QueryRunner::class)->databases($connection))->toBe([]);
});

it('says so when the server offers no databases to pick', function () {
    $browser = onSqlite();

    // A server connection that answers with nothing: the picker stays shut and
    // the status line says why, rather than opening an empty list.
    $browser->connection = new Connection([
        'name' => 'server', 'driver' => 'mysql',
        'host' => '127.0.0.1', 'port' => 1, 'database' => '',
    ]);

    $browser->openDatabases();

    expect($browser->databasePicker)->toBeNull()
        ->and($browser->status)->toContain('no databases');
});

function onServer(Browser $browser, array $attributes = []): object
{
    $runner = new class(app(ConnectionManager::class)) extends QueryRunner
    {
        public array $created = [];

        public array $dropped = [];

        public array $droppedFrom = [];

        public bool $refuse = false;

        public function dropDatabase(Connection $connection, string $name): QueryResult
        {
            if ($this->refuse) {
                return new QueryResult(rows: [], durationMs: 0, error: 'database is being accessed by other users');
            }

            $this->dropped[] = $name;
            $this->droppedFrom[] = $connection->activeDatabase();
            $this->databases = array_values(array_diff($this->databases, [$name]));

            return new QueryResult(rows: [], durationMs: 0);
        }

        public array $databases = ['shop'];

        public function databases(Connection $connection): array
        {
            return $this->databases;
        }

        public function createDatabase(Connection $connection, string $name): QueryResult
        {
            $this->created[] = $name;

            return new QueryResult(rows: [], durationMs: 0);
        }

        public function tables(Connection $connection): array
        {
            return [];
        }
    };

    (new ReflectionProperty($browser, 'runner'))->setValue($browser, $runner);

    $browser->connection = new Connection([
        'name' => 'server', 'driver' => 'mysql', 'host' => '127.0.0.1',
        'port' => 3306, 'database' => 'shop', ...$attributes,
    ]);

    return $runner;
}

function command(Browser $browser, string $command): void
{
    foreach (mb_str_split(':'.$command) as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\n");
}

it('creates a database and switches to it', function () {
    $browser = onSqlite();
    $runner = onServer($browser);

    command($browser, 'create database kmstools');

    expect($runner->created)->toBe(['kmstools'])
        ->and($browser->connection->activeDatabase())->toBe('kmstools')
        ->and($browser->status)->toBe('created kmstools · using it');
});

it('takes the name as typed, quotes and all', function () {
    $browser = onSqlite();
    $runner = onServer($browser);

    command($browser, 'create database `kmstools`');

    expect($runner->created)->toBe(['kmstools']);
});

it('asks for the name when none is given', function () {
    $browser = onSqlite();
    $runner = onServer($browser);

    command($browser, 'create database');

    expect($runner->created)->toBe([])
        ->and($browser->command)->toBe('create database ');
});

it('will not create a database on a read-only connection', function () {
    $browser = onSqlite();
    $runner = onServer($browser, ['read_only' => true]);

    command($browser, 'create database kmstools');

    expect($runner->created)->toBe([])
        ->and($browser->status)->toBe('this connection is marked read-only')
        ->and($browser->connection->activeDatabase())->toBe('shop');
});

it('will not create a database on sqlite', function () {
    $browser = onSqlite();

    command($browser, 'create database kmstools');

    expect($browser->status)->toContain('one file');
});

it('shows the error when the server refuses', function () {
    $browser = onSqlite();
    $browser->connection = new Connection([
        'name' => 'unreachable', 'driver' => 'mysql',
        'host' => '127.0.0.1', 'port' => 1, 'database' => 'shop',
    ]);

    command($browser, 'create database kmstools');

    expect($browser->problem)->not->toBeNull()
        ->and($browser->connection->activeDatabase())->toBe('shop');
});

it('offers to create a database from the palette', function () {
    $browser = onSqlite();
    onServer($browser);

    $browser->emit('key', "\x0b");

    foreach (mb_str_split('create a database') as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\n");

    expect($browser->command)->toBe('create database ');
});

it('types j, k and q into the database list rather than moving or closing it', function () {
    $browser = onSqlite();
    onServer($browser);
    onServer($browser);
    $browser->databasePicker = new Picker('DATABASE', ['jobs', 'kmstools', 'queue']);

    foreach (mb_str_split('kmsq') as $char) {
        $browser->emit('key', $char);
    }

    expect($browser->databasePicker)->not->toBeNull()
        ->and($browser->databasePicker->query->buffer())->toBe('kmsq');

    $browser->emit('key', "\x7f");
    $browser->emit('key', "\n");

    expect($browser->connection->activeDatabase())->toBe('kmstools');
});

it('moves through the database list with the arrows and ctrl+n', function () {
    $browser = onSqlite();
    $browser->databasePicker = new Picker('DATABASE', ['jobs', 'kmstools', 'queue']);

    $browser->emit('key', "\x0e");
    $browser->emit('key', "\e[B");

    expect($browser->databasePicker->selected())->toBe('queue');

    $browser->emit('key', "\e[A");

    expect($browser->databasePicker->selected())->toBe('kmstools');
});

it('offers to create the database typed into the list when there is none by that name', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $browser->databasePicker = new Picker('DATABASE', ['jobs', 'shop'], 'shop', creates: true);

    foreach (mb_str_split('kmstools') as $char) {
        $browser->emit('key', $char);
    }

    $frame = preg_replace('/\e\[[0-9;]*m/', '', (new ReflectionMethod($browser, 'renderTheme'))->invoke($browser));

    expect($frame)->toContain('+ create kmstools')
        ->and($browser->databasePicker->creating())->toBeTrue();

    $browser->emit('key', "\n");

    expect($runner->created)->toBe(['kmstools'])
        ->and($browser->databasePicker)->toBeNull()
        ->and($browser->connection->activeDatabase())->toBe('kmstools');
});

it('picks a matching database before offering to create one', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $browser->databasePicker = new Picker('DATABASE', ['jobs', 'shop'], 'shop', creates: true);

    foreach (mb_str_split('jo') as $char) {
        $browser->emit('key', $char);
    }

    $browser->emit('key', "\n");

    expect($runner->created)->toBe([])
        ->and($browser->connection->activeDatabase())->toBe('jobs');
});

it('does not offer to create a database that is already there', function () {
    $picker = new Picker('DATABASE', ['jobs', 'shop'], creates: true);
    $picker->type('s');
    $picker->type('h');
    $picker->type('o');
    $picker->type('p');

    expect($picker->newOption())->toBeNull();
});

it('opens a list that can create on a writable server only', function () {
    $browser = onSqlite();
    onServer($browser);
    $browser->openDatabases();

    expect($browser->databasePicker->creates)->toBeTrue();

    onServer($browser, ['read_only' => true]);
    $browser->openDatabases();

    expect($browser->databasePicker->creates)->toBeFalse();
});

it('drops a database', function () {
    $browser = onSqlite();
    $runner = onServer($browser);

    command($browser, 'drop database kmstools');

    expect($runner->dropped)->toBe(['kmstools'])
        ->and($browser->status)->toBe('dropped kmstools')
        ->and($browser->connection->activeDatabase())->toBe('shop');
});

it('opens the database list after dropping the one in use', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $runner->databases = ['jobs', 'kmstools', 'shop'];
    $browser->connection->sessionDatabase = 'kmstools';

    command($browser, 'drop database kmstools');

    expect($runner->dropped)->toBe(['kmstools'])
        ->and($runner->droppedFrom)->toBe(['shop'])
        ->and($browser->connection->activeDatabase())->toBe('')
        ->and($browser->tables)->toBe([])
        ->and($browser->databasePicker?->options)->toBe(['jobs', 'shop'])
        ->and($browser->status)->toBe('dropped kmstools');
});

it('drops the saved database from another of yours', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $runner->databases = ['information_schema', 'mysql', 'kmstools', 'shop'];

    command($browser, 'drop database shop');

    expect($runner->droppedFrom)->toBe(['kmstools']);
});

it('drops from a system database when there is nothing else', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $runner->databases = ['information_schema', 'shop'];

    command($browser, 'drop database shop');

    expect($runner->droppedFrom)->toBe(['information_schema']);
});

it('stays on the database when dropping it fails', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $runner->databases = ['jobs', 'shop'];
    $runner->refuse = true;

    command($browser, 'drop database shop');

    expect($browser->problem)->not->toBeNull()
        ->and($browser->connection->activeDatabase())->toBe('shop')
        ->and($browser->databasePicker)->toBeNull();
});

it('keeps the database in use when there is nowhere to move to', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $runner->databases = ['shop'];

    command($browser, 'drop database shop');

    expect($runner->dropped)->toBe([])
        ->and($browser->connection->activeDatabase())->toBe('shop')
        ->and($browser->status)->toContain('no other database to drop it from');
});

it('will not drop a database on a read-only connection', function () {
    $browser = onSqlite();
    $runner = onServer($browser, ['read_only' => true]);

    command($browser, 'drop database kmstools');

    expect($runner->dropped)->toBe([])
        ->and($browser->status)->toBe('this connection is marked read-only');
});

it('asks before dropping a database from the list', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $browser->databasePicker = new Picker('DATABASE', ['kmstools', 'shop'], 'shop', creates: true);

    $browser->emit('key', "\e[A");
    $browser->emit('key', "\x04");

    expect($runner->dropped)->toBe([])
        ->and($browser->databasePicker)->toBeNull()
        ->and($browser->command)->toBe('drop database kmstools');

    $browser->emit('key', "\n");

    expect($runner->dropped)->toBe(['kmstools']);
});

it('drops nothing when the confirmation is cancelled', function () {
    $browser = onSqlite();
    $runner = onServer($browser);
    $browser->databasePicker = new Picker('DATABASE', ['kmstools', 'shop'], 'kmstools', creates: true);

    $browser->emit('key', "\x04");
    $browser->emit('key', "\e");

    expect($runner->dropped)->toBe([])
        ->and($browser->command)->toBeNull();
});

it('creates and drops a sql server database from master', function () {
    $manager = new class extends ConnectionManager
    {
        public array $databases = [];

        public function resolve(Connection $connection): Illuminate\Database\Connection
        {
            $this->databases[] = $connection->activeDatabase();

            throw new RuntimeException('no server here');
        }
    };

    $connection = new Connection([
        'name' => 'mssql', 'driver' => 'sqlsrv', 'host' => '127.0.0.1', 'database' => 'shop',
    ]);

    $runner = new QueryRunner($manager);
    $runner->createDatabase($connection, 'kmstools');
    $runner->dropDatabase($connection, 'kmstools');

    expect($manager->databases)->toBe(['master', 'master'])
        ->and($connection->activeDatabase())->toBe('shop');
});

it('creates a database from the one in use everywhere else', function () {
    $manager = new class extends ConnectionManager
    {
        public array $databases = [];

        public function resolve(Connection $connection): Illuminate\Database\Connection
        {
            $this->databases[] = $connection->activeDatabase();

            throw new RuntimeException('no server here');
        }
    };

    $connection = new Connection([
        'name' => 'pg', 'driver' => 'pgsql', 'host' => '127.0.0.1', 'database' => 'shop',
    ]);

    (new QueryRunner($manager))->createDatabase($connection, 'kmstools');

    expect($manager->databases)->toBe(['shop']);
});
