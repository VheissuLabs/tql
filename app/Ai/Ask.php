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
        if (! $this->configured()) {
            return 'no AI provider configured — set '.strtoupper((string) config('tql.ai.provider')).
                '_API_KEY, or change [ai] provider in config.toml';
        }

        $agent = new SqlWriter(
            driver: (string) $connection->driver,
            schema: $this->schema->for($connection, $table),
        );

        try {
            $response = $agent->prompt(
                $question,
                provider: (string) config('tql.ai.provider'),
                model: (string) config('tql.ai.model'),
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
        $provider = (string) config('tql.ai.provider');

        return (string) config("ai.providers.{$provider}.key") !== '';
    }
}
