<?php

namespace App\Ai;

use App\Ai\Agents\SqlWriter;
use App\Models\Connection;
use Throwable;

class Ask
{
    public function __construct(private SchemaSummary $schema) {}

    /**
     * Ask for a query. Returns the SQL to put in the editor, or a message
     * saying why there is none.
     *
     * @return array{query: string, explanation: string, notes: string}|string
     */
    public function for(Connection $connection, string $question, ?string $table = null): array|string
    {
        $provider = Providers::chosen();

        if ($provider === null) {
            return $this->missing();
        }

        $agent = new SqlWriter(
            driver: (string) $connection->driver,
            schema: $this->schema->for($connection, $table),
        );

        try {
            $response = $agent->prompt(
                $question,
                provider: $provider,
                model: Providers::model($provider),
                timeout: (int) config('tql.ai.timeout'),
            );
        } catch (Throwable $e) {
            return 'the model could not be reached: '.$e->getMessage();
        }

        $answer = $this->decode($response);

        if ($answer === null) {
            return 'the model did not answer with a query';
        }

        return $answer;
    }

    /**
     * @return array{query: string, explanation: string, notes: string}|null
     */
    private function decode(mixed $response): ?array
    {
        $data = is_array($response) ? $response : json_decode((string) $response, true);

        if (! is_array($data) || ! isset($data['query'])) {
            return null;
        }

        return [
            'query' => trim((string) $data['query']),
            'explanation' => trim((string) ($data['explanation'] ?? '')),
            'notes' => trim((string) ($data['notes'] ?? '')),
        ];
    }

    public function configured(): bool
    {
        return Providers::chosen() !== null;
    }

    /**
     * Name the variable to set. "No provider configured" tells a user nothing
     * about what to do next.
     */
    private function missing(): string
    {
        $configured = trim((string) config('tql.ai.provider', 'auto'));

        if ($configured !== '' && $configured !== 'auto') {
            return "no key for {$configured} — set ".Providers::keyVariable($configured)
                .', or point [ai] url at a local model';
        }

        return 'no model to ask — set an api key such as ANTHROPIC_API_KEY or '.
            'OPENAI_API_KEY, or point [ai] url at a local endpoint like LM Studio';
    }
}
