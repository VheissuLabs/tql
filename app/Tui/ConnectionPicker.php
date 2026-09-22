<?php

namespace App\Tui;

use App\Models\Connection;
use App\Prompts\Renderers\ConnectionPickerRenderer;
use Chewie\Concerns\CreatesAnAltScreen;
use Chewie\Concerns\RegistersRenderers;
use Illuminate\Support\Collection;
use Laravel\Prompts\Key;
use Laravel\Prompts\Prompt;

class ConnectionPicker extends Prompt
{
    use CreatesAnAltScreen;
    use RegistersRenderers;

    public int $index = 0;

    public ?string $command = null;

    public ?string $status = null;

    private ?string $choice = null;

    public function __construct(public Collection $connections)
    {
        $this->registerRenderer(ConnectionPickerRenderer::class);

        $this->createAltScreen();

        $this->on('key', fn (string $key) => $this->onKey($key));
    }

    public function value(): mixed
    {
        return $this->choice;
    }

    public function rows(): array
    {
        return $this->connections
            ->map(fn (Connection $c) => [
                'name' => $c->name,
                'driver' => $c->driver,
                'where' => $c->describe(),
                'used' => $c->last_used_at?->diffForHumans() ?? 'never',
            ])
            ->all();
    }

    public function onKey(string $key): void
    {
        if ($this->command !== null) {
            $this->handleCommandKey($key);

            return;
        }

        match (true) {
            $key === ':' => $this->openCommandLine(),
            $key === 'q', $key === Key::ESCAPE => $this->finish('quit'),
            $key === 'n' => $this->finish('new'),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->move(1),
            $key === Key::ENTER => $this->select(),
            default => true,
        };
    }

    private function select(): bool
    {
        $connection = $this->connections->values()->get($this->index);

        return $this->finish($connection === null ? 'quit' : (string) $connection->id);
    }

    private function finish(string $choice): bool
    {
        $this->choice = $choice;
        $this->state = 'submit';

        return false;
    }

    private function move(int $by): bool
    {
        $this->index = max(0, min($this->connections->count() - 1, $this->index + $by));

        return true;
    }

    private function openCommandLine(): bool
    {
        $this->command = '';

        return true;
    }

    private function handleCommandKey(string $key): bool
    {
        if ($key === Key::ESCAPE) {
            $this->command = null;

            return true;
        }

        if ($key === Key::ENTER) {
            $command = trim($this->command ?? '');
            $this->command = null;

            return match ($command) {
                'q', 'q!', 'quit' => $this->finish('quit'),
                'new' => $this->finish('new'),
                default => $this->unknown($command),
            };
        }

        if (in_array($key, [Key::BACKSPACE, Key::CTRL_H], true)) {
            $this->command = mb_substr($this->command, 0, -1);

            return true;
        }

        if (mb_strlen($key) === 1 && ! ctype_cntrl($key)) {
            $this->command .= $key;
        }

        return true;
    }

    private function unknown(string $command): bool
    {
        $this->status = "unknown command: :{$command}";

        return true;
    }
}
