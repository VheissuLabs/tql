<?php

namespace App\Commands;

use App\Commands\Concerns\AnswersAgents;
use App\Database\AgentAccess;
use LaravelZero\Framework\Commands\Command;

class DescribeCommand extends Command
{
    use AnswersAgents;

    protected $signature = 'describe
        {connection : The connection, by name}
        {table : The table}
        {--database= : Another database on the same server}
        {--json : JSON, which is what you get when the output is piped}
        {--table : A table, which is what you get in a terminal}';

    protected $description = 'Describe a table\'s columns, for a script or an agent';

    public function handle(AgentAccess $access): int
    {
        return $this->answer(
            fn () => $access->describe($this->argument('connection'), $this->argument('table'), $this->option('database')),
            fn (array $answer) => $this->table(
                ['Column', 'Type', 'Null', 'Default', 'Key'],
                array_map(fn (array $column) => [
                    $column['name'],
                    $column['type'] ?? $column['type_name'] ?? '',
                    ($column['nullable'] ?? false) ? 'yes' : 'no',
                    $column['default'] ?? '',
                    $column['name'] === $answer['primary_key'] ? 'primary' : '',
                ], $answer['columns']),
            ),
        );
    }
}
