<?php

namespace App\Commands;

use App\Commands\Concerns\AnswersAgents;
use App\Database\AgentAccess;
use LaravelZero\Framework\Commands\Command;

class QueryCommand extends Command
{
    use AnswersAgents;

    protected $signature = 'query
        {connection : The connection, by name}
        {sql? : The statement; - or nothing reads it from stdin}
        {--database= : Another database on the same server}
        {--limit=200 : The most rows to return}
        {--json : JSON, which is what you get when the output is piped}
        {--table : A table, which is what you get in a terminal}';

    protected $description = 'Run a read-only query, for a script or an agent';

    public function handle(AgentAccess $access): int
    {
        $statement = $this->statement();

        if ($statement === null) {
            return $this->refuse('Give the statement as an argument, or pipe it in: echo "select 1" | tql query <connection>');
        }

        return $this->answer(
            fn () => $access->query($this->argument('connection'), $statement, 'cli', $this->option('database'), (int) $this->option('limit')),
            fn (array $answer) => $this->draw($answer),
        );
    }

    private function statement(): ?string
    {
        $given = $this->argument('sql');

        if ($given !== null && $given !== '-') {
            return $given;
        }

        if (function_exists('stream_isatty') && @stream_isatty(STDIN)) {
            return null;
        }

        $piped = trim((string) stream_get_contents(STDIN));

        return $piped === ''
            ? null
            : $piped;
    }

    private function draw(array $answer): void
    {
        $rows = array_map(fn (array|object $row) => (array) $row, $answer['results']);

        if ($rows !== []) {
            $this->table(
                array_keys($rows[0]),
                array_map(fn (array $row) => array_map(fn (mixed $value) => $value ?? 'NULL', $row), $rows),
            );
        }

        $this->line($answer['rows'].($answer['rows'] === 1 ? ' row' : ' rows').' · '.$answer['duration_ms'].'ms'.($answer['truncated'] ? ' · showing the first '.count($rows) : ''));
    }
}
