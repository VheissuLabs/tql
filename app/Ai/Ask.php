<?php

namespace App\Ai;

use App\Ai\Agents\SqlWriter;
use App\Models\Connection;
use Illuminate\Http\Client\Response;
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
            $data = static::fromText(static::reasoning($response));
        }

        if (! is_array($data) || ! isset($data['query'])) {
            return null;
        }

        return [
            'query' => trim((string) $data['query']),
            'explanation' => trim((string) ($data['explanation'] ?? '')),
            'notes' => trim((string) ($data['notes'] ?? '')),
        ];
    }

    /**
     * What a reasoning model said while it was thinking.
     *
     * Local endpoints serving a reasoning model — qwen3 through LM Studio, for
     * one — put the whole answer in reasoning_content and leave content empty,
     * so the structured result arrives blank even though the model answered.
     */
    private static function reasoning(mixed $response): string
    {
        $step = is_object($response) && isset($response->steps) ? $response->steps->first() : null;

        if (is_string($step->reasoning ?? null) && trim($step->reasoning) !== '') {
            return $step->reasoning;
        }

        $raw = $step->raw ?? null;

        if (! $raw instanceof Response) {
            return '';
        }

        try {
            return (string) data_get($raw->json(), 'choices.0.message.reasoning_content', '');
        } catch (Throwable) {
            return '';
        }
    }

    /**
     * The first JSON object in a piece of prose, for a model that wrapped its
     * answer in words or in <think> tags.
     *
     * @return array<string, mixed>|null
     */
    private static function fromText(string $text): ?array
    {
        if ($text === '' || preg_match('/\{.*\}/s', $text, $match) !== 1) {
            return null;
        }

        $data = json_decode($match[0], true);

        return is_array($data) ? $data : null;
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
