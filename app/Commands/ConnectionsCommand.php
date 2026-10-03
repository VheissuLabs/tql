<?php

namespace App\Commands;

use App\Commands\Concerns\AnswersAgents;
use App\Database\AgentAccess;
use LaravelZero\Framework\Commands\Command;

class ConnectionsCommand extends Command
{
    use AnswersAgents;

    protected $signature = 'connections
        {--json : JSON, which is what you get when the output is piped}
        {--table : A table, which is what you get in a terminal}';

    protected $description = 'List the saved connections, for a script or an agent';

    public function handle(AgentAccess $access): int
    {
        return $this->answer(
            fn () => $access->connections(),
            fn (array $connections) => $this->table(
                ['Name', 'Driver', 'Where', 'Read only', 'Last used'],
                array_map(fn (array $connection) => [
                    $connection['name'],
                    $connection['driver'],
                    $connection['target'],
                    $connection['read_only'] ? 'yes' : '',
                    $connection['last_used_at'] ?? 'never',
                ], $connections),
            ),
        );
    }
}
