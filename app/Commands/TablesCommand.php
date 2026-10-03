<?php

namespace App\Commands;

use App\Commands\Concerns\AnswersAgents;
use App\Database\AgentAccess;
use LaravelZero\Framework\Commands\Command;

class TablesCommand extends Command
{
    use AnswersAgents;

    protected $signature = 'tables
        {connection : The connection, by name}
        {--database= : Another database on the same server}
        {--json : JSON, which is what you get when the output is piped}
        {--table : A table, which is what you get in a terminal}';

    protected $description = 'List the tables in a connection, for a script or an agent';

    public function handle(AgentAccess $access): int
    {
        return $this->answer(
            fn () => $access->tables($this->argument('connection'), $this->option('database')),
            fn (array $answer) => array_map($this->line(...), $answer['tables']),
        );
    }
}
