<?php

namespace App\Ai;

class Providers
{
    /**
     * A sensible text model per provider, used when the config does not name
     * one. Anything here can be overridden with [ai] model.
     */
    public const MODELS = [
        'anthropic' => 'claude-sonnet-5',
        'openai' => 'gpt-5',
        'gemini' => 'gemini-2.5-pro',
        'groq' => 'llama-3.3-70b-versatile',
        'mistral' => 'mistral-large-latest',
        'deepseek' => 'deepseek-chat',
        'xai' => 'grok-4',
        'openrouter' => 'anthropic/claude-sonnet-5',
        'ollama' => 'llama3.2',
        'azure' => 'gpt-4o',
        'bedrock' => 'anthropic.claude-sonnet-4-20250514-v1:0',
        'openai-compatible' => '',
    ];

    public static function describe(string $provider): string
    {
        return $provider === self::LOCAL
            ? (string) config('ai.providers.'.self::LOCAL.'.url')
            : $provider;
    }

    /** The provider registered from [ai] url, for a local endpoint. */
    public const LOCAL = 'tql-local';

    /**
     * Register an OpenAI-compatible endpoint from [ai] url, so pointing tql at
     * LM Studio or any local gateway is two lines of config rather than an
     * edit to a PHP file.
     */
    public static function registerLocal(): void
    {
        $url = trim((string) config('tql.ai.url', ''));

        if ($url === '') {
            return;
        }

        config(['ai.providers.'.self::LOCAL => array_filter([
            'driver' => 'openai-compatible',
            'url' => $url,
            'key' => trim((string) config('tql.ai.key', '')),
            'models' => array_filter([
                'text' => array_filter(['default' => trim((string) config('tql.ai.model', ''))]),
            ]),
        ])]);
    }

    /**
     * Providers that can answer: they are configured in the AI SDK and a key
     * is present. Ollama is local, so it counts without one.
     *
     * @return array<int, string>
     */
    public static function available(): array
    {
        $available = [];

        foreach (array_keys((array) config('ai.providers', [])) as $provider) {
            if (static::hasKey((string) $provider)) {
                $available[] = (string) $provider;
            }
        }

        return $available;
    }

    public static function hasKey(string $provider): bool
    {
        // A local endpoint needs a url, not a key.
        if ($provider === self::LOCAL) {
            return trim((string) config('ai.providers.'.self::LOCAL.'.url', '')) !== '';
        }

        if ($provider === 'ollama') {
            return (string) config('ai.providers.ollama.url', '') !== '';
        }

        return trim((string) config("ai.providers.{$provider}.key", '')) !== '';
    }

    /**
     * The provider to ask. "auto" takes the first one with a key, in the
     * order the AI SDK lists them, so a user with any single key configured
     * gets a working "a" without touching config.toml.
     */
    public static function chosen(): ?string
    {
        static::registerLocal();

        $configured = trim((string) config('tql.ai.provider', 'auto'));

        if ($configured !== '' && $configured !== 'auto') {
            if (static::hasKey($configured)) {
                return $configured;
            }

            // A named provider with no key is usually a default nobody
            // changed; a url was typed on purpose, so it answers instead.
            return static::hasKey(self::LOCAL) ? self::LOCAL : null;
        }

        // A url points somewhere deliberate, so it wins over any stray key.
        if (static::hasKey(self::LOCAL)) {
            return self::LOCAL;
        }

        return static::available()[0] ?? null;
    }

    public static function model(string $provider): string
    {
        $configured = trim((string) config('tql.ai.model', ''));

        return $configured !== '' ? $configured : (self::MODELS[$provider] ?? '');
    }

    /**
     * The environment variable a provider takes its key from, so the app can
     * say which one is missing rather than just "not configured".
     */
    public static function keyVariable(string $provider): string
    {
        return match ($provider) {
            'azure' => 'AZURE_OPENAI_API_KEY',
            'bedrock' => 'AWS_BEARER_TOKEN_BEDROCK',
            'eleven' => 'ELEVENLABS_API_KEY',
            'openai-compatible' => 'OPENAI_COMPATIBLE_API_KEY',
            default => strtoupper(str_replace('-', '_', $provider)).'_API_KEY',
        };
    }
}
