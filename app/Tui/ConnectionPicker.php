<?php

namespace App\Tui;

use App\Models\Connection;
use App\Prompts\Renderers\ConnectionPickerRenderer;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RendersSmoothly;
use Chewie\Concerns\CreatesAnAltScreen;
use Chewie\Concerns\RegistersRenderers;
use Illuminate\Support\Collection;
use Laravel\Prompts\Key;
use Laravel\Prompts\Prompt;

class ConnectionPicker extends Prompt
{
    use CreatesAnAltScreen;
    use HandlesMouse;
    use RegistersRenderers;
    use RendersSmoothly;

    /** ctrl+s, matching the value editor in the browser. */
    public const SAVE = "\x13";

    public int $index = 0;

    public ?string $command = null;

    public ?string $status = null;

    public int $start = 0;

    public ?int $firstBodyRow = null;

    public ?ConnectionForm $form = null;

    /**
     * Connection ids marked for deletion. Same as the grid: nothing goes until
     * :w, so a stray d costs a keystroke rather than a saved connection.
     *
     * @var array<int, int>
     */
    public array $pendingDeletes = [];

    private ?string $choice = null;

    public function __construct(public Collection $connections)
    {
        $this->registerRenderer(ConnectionPickerRenderer::class);

        $this->createAltScreen();

        $this->enableMouse();

        $this->on('key', fn (string $key) => $this->onKey($key));
    }

    public function __destruct()
    {
        $this->disableMouse();

        $this->exitAltScreen();

        parent::__destruct();
    }

    public function value(): mixed
    {
        return $this->choice;
    }

    public function shape(): string
    {
        return ($this->form === null ? 'list' : 'form').'|'.count($this->connections);
    }

    public function rows(): array
    {
        return $this->connections
            ->map(fn (Connection $c) => [
                'id' => $c->id,
                'name' => $c->name,
                'driver' => $c->driver,
                'where' => $c->describe(),
                'used' => $c->last_used_at?->diffForHumans() ?? 'never',
            ])
            ->all();
    }

    public function onKey(string $key): void
    {
        if ($event = Mouse::parse($key)) {
            $this->onMouse($event);

            return;
        }

        if ($this->form !== null) {
            $this->handleFormKey($key);

            return;
        }

        if ($this->command !== null) {
            $this->handleCommandKey($key);

            return;
        }

        match (true) {
            $key === ':' => $this->openCommandLine(),
            $key === 'q', $key === Key::ESCAPE => $this->quit(),
            $key === 'n' => $this->create(),
            $key === 'e' => $this->edit(),
            $key === 'd' => $this->markDelete(),
            $key === 'u' => $this->unmarkAll(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->move(1),
            $key === Key::ENTER => $this->select(),
            default => true,
        };
    }

    private function onMouse(array $event): void
    {
        if (! $event['pressed']) {
            return;
        }

        match ($event['button']) {
            Mouse::WHEEL_UP => $this->move(-1),
            Mouse::WHEEL_DOWN => $this->move(1),
            Mouse::LEFT => $this->clickRow($event['row']),
            default => true,
        };
    }

    private function clickRow(int $row): bool
    {
        $index = Layout::bodyIndex($row, $this->firstBodyRow ?? Layout::firstBodyRow(2));

        if ($index === null) {
            return true;
        }

        $target = $this->start + $index;

        if ($target < 0 || $target >= $this->connections->count()) {
            return true;
        }

        $this->index = $target;

        return $this->select();
    }

    private function markDelete(): bool
    {
        $connection = $this->connections->values()->get($this->index);

        if ($connection === null) {
            return true;
        }

        $at = array_search($connection->id, $this->pendingDeletes, true);

        if ($at === false) {
            $this->pendingDeletes[] = $connection->id;
        } else {
            unset($this->pendingDeletes[$at]);
            $this->pendingDeletes = array_values($this->pendingDeletes);
        }

        $this->move(1);

        $count = count($this->pendingDeletes);

        $this->status = $count === 0
            ? 'marks cleared'
            : $count.' marked for deletion · :w writes · u clears';

        return true;
    }

    private function unmarkAll(): bool
    {
        $this->pendingDeletes = [];
        $this->status = 'marks cleared';

        return true;
    }

    public function writePending(): bool
    {
        if ($this->pendingDeletes === []) {
            $this->status = 'nothing to write';

            return true;
        }

        $count = count($this->pendingDeletes);

        Connection::whereIn('id', $this->pendingDeletes)->delete();

        $this->pendingDeletes = [];
        $this->connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();
        $this->index = max(0, min($this->index, $this->connections->count() - 1));

        $this->status = 'deleted '.$count.' connection'.($count === 1 ? '' : 's');

        return true;
    }

    public function isMarked(int $id): bool
    {
        return in_array($id, $this->pendingDeletes, true);
    }

    private function create(): bool
    {
        $this->form = new ConnectionForm(new Connection(['driver' => 'sqlite']), creating: true);
        $this->status = null;

        return true;
    }

    private function edit(): bool
    {
        $connection = $this->connections->values()->get($this->index);

        if ($connection !== null) {
            $this->form = new ConnectionForm($connection);
            $this->status = null;
        }

        return true;
    }

    private function handleFormKey(string $key): void
    {
        $form = $this->form;

        if ($form->editing) {
            $this->handleFieldKey($form, $key);

            return;
        }

        // The driver is a fixed set, so it cycles rather than being typed.
        if ($form->currentKey() === 'driver') {
            match (true) {
                $key === Key::ESCAPE, $key === 'q' => $this->closeForm('nothing changed'),
                $key === self::SAVE => $this->saveForm(),
                in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->move(-1),
                in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->move(1),
                in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $form->cycleDriver(-1),
                in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l', ' '], true), $key === Key::ENTER => $form->cycleDriver(),
                default => true,
            };

            return;
        }

        match (true) {
            $key === Key::ESCAPE, $key === 'q' => $this->closeForm('nothing changed'),
            $key === self::SAVE => $this->saveForm(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->move(1),
            $key === Key::ENTER, $key === 'i' => $form->start(),
            default => true,
        };
    }

    private function handleFieldKey(ConnectionForm $form, string $key): void
    {
        // Saving from inside a field keeps what you just typed, rather than
        // making you press enter first and wonder why nothing happened.
        if ($key === self::SAVE) {
            $form->commit();
            $this->saveForm();

            return;
        }

        if ($key === Key::ESCAPE) {
            $form->abandon();

            return;
        }

        if ($key === Key::ENTER) {
            $form->commit();

            return;
        }

        // Everything else is ordinary line editing: arrows, home, end,
        // backspace, delete and typing, cursor and all.
        $form->editor?->handle($key);
    }

    private function saveForm(): void
    {
        $error = $this->form->save();

        if ($error !== null) {
            $this->form->error = $error;

            return;
        }

        $connection = $this->form->connection;
        $creating = $this->form->creating;

        $this->connections = Connection::orderByDesc('last_used_at')->orderBy('name')->get();

        if ($creating) {
            $this->index = max(0, $this->connections->search(
                fn (Connection $c) => $c->id === $connection->id,
            ) ?: 0);
        }

        $this->closeForm(($creating ? 'added ' : 'saved ').$connection->name);
    }

    private function closeForm(string $status): void
    {
        $this->form = null;
        $this->status = $status;
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
                'q', 'q!', 'quit' => $this->quit(),
                'w', 'write' => $this->writePending(),
                'new' => $this->create(),
                default => $this->unknown($command),
            };
        }

        if (in_array($key, [Key::BACKSPACE, Key::CTRL_H], true)) {
            $this->command = mb_substr($this->command, 0, -1);

            return true;
        }

        $this->command .= Input::text($key);

        return true;
    }

    /**
     * Unwritten marks are dropped and said out loud rather than being lost in
     * silence or written on the way out.
     */
    private function quit(): bool
    {
        if ($this->pendingDeletes !== []) {
            $count = count($this->pendingDeletes);

            $this->pendingDeletes = [];
            $this->status = $count.' unwritten mark'.($count === 1 ? '' : 's').
                ' dropped — quit again to leave';

            return true;
        }

        return $this->finish('quit');
    }

    private function unknown(string $command): bool
    {
        $this->status = "unknown command: :{$command}";

        return true;
    }
}
