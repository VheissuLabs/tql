<?php

namespace App\Ai\Agents;

use Illuminate\Contracts\JsonSchema\JsonSchema;
use Laravel\Ai\Contracts\Agent;
use Laravel\Ai\Contracts\HasStructuredOutput;
use Laravel\Ai\Promptable;
use Stringable;

/**
 * Turns a question into SQL for the database you are looking at.
 *
 * It writes the query into the editor and explains it; it never runs it.
 * Reviewing the statement before it touches the database is the point, and
 * seeing the SQL is how you end up learning it.
 */
class SqlWriter implements Agent, HasStructuredOutput
{
    use Promptable;

    public function __construct(
        public string $driver,
        public string $schema,
    ) {}

    public function instructions(): Stringable|string
    {
        return <<<PROMPT
        You write a single SQL query for a {$this->driver} database, for someone
        who knows Eloquent but not much SQL.

        Rules:
        - Return exactly one statement. Never several, never a transaction.
        - Only read data: select, with, show, explain. Never insert, update,
          delete, drop, alter, truncate, grant or create. If the question asks
          for a write, explain in the notes what it would take and return a
          select that shows the rows it would affect.
        - Use only the tables and columns in the schema below. If the question
          cannot be answered with them, say so in the notes and return an empty
          query rather than inventing a column.
        - Add a limit unless the question is an aggregate.
        - Quote identifiers the way {$this->driver} does.
        - Write it over several lines so it can be read.

        The explanation is for someone learning SQL. Say what each clause is
        doing in plain words, in two or three sentences. No preamble.

        The schema:

        {$this->schema}
        PROMPT;
    }

    public function schema(JsonSchema $schema): array
    {
        return [
            'query' => $schema->string()->required(),
            'explanation' => $schema->string()->required(),
            'notes' => $schema->string(),
        ];
    }
}
