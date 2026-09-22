<?php

namespace App\Commands;

use App\Database\ConnectionManager;
use App\Database\QueryRunner;
use App\Models\Connection;
use App\Tui\Browser;
use App\Tui\ConnectionPicker;
use App\Tui\RowFormatter;
use LaravelZero\Framework\Commands\Command;

use function Laravel\Prompts\error;
use function Laravel\Prompts\pause;

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

        return match ($choice) {
            'quit', null => null,
            default => Connection::find((int) $choice),
        };
    }
}
