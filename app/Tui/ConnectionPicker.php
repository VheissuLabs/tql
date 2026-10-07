<?php

namespace App\Tui;

use App\Models\Connection;
use App\Models\Setting;
use App\Prompts\Renderers\ConnectionPickerRenderer;
use App\Tui\Concerns\HandlesMouse;
use App\Tui\Concerns\RedrawsOnResize;
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
    use RedrawsOnResize;
    use RegistersRenderers;
    use RendersSmoothly;

    /** ctrl+s, matching the value editor in the browser. */
    public const SAVE = "\x13";

    public int $index = 0;

    public ?string $command = null;

    /**
     * The line under the hotkeys. It fades after a few seconds, the same as
     * it does in the browser, back to the connection count.
     */
    public ?string $status = null {
        set(?string $value) {
            if ($value !== $this->status) {
                $this->statusSince = microtime(true);
            }

            $this->status = $value;
        }
    }

    public float $statusSince = 0.0;

    public ?string $notice = null;

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

    /**
     * Groups folded shut. Remembered between runs, since the point of folding
     * one is not having to look at it every time.
     *
     * @var array<int, string>
     */
    public array $collapsed = [];

    public const COLLAPSED = 'connections.collapsed_groups';

    private ?string $choice = null;

    public function __construct(public Collection $connections)
    {
        $this->registerRenderer(ConnectionPickerRenderer::class);

        $this->collapsed = json_decode(Setting::read(self::COLLAPSED) ?? '[]', true) ?: [];

        // Start on the connection used last, wherever its group put it.
        $this->index = $this->indexOf($connections->first()?->id) ?? 0;

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

    public function fadeStatus(?float $now = null): bool
    {
        $after = Layout::statusSeconds();

        if ($after <= 0 || $this->status === null || $this->status === '') {
            return false;
        }

        if (($now ?? microtime(true)) - $this->statusSince < $after) {
            return false;
        }

        $this->status = null;

        return true;
    }

    public function idle(bool $timedOut): bool
    {
        $redraw = $this->resized;
        $this->resized = false;

        return ($timedOut && $this->fadeStatus()) || $redraw;
    }

    public function value(): mixed
    {
        return $this->choice;
    }

    public function shape(): string
    {
        return ($this->form === null ? 'list' : 'form').'|'.count($this->rows());
    }

    /**
     * What the list shows, top to bottom: connections without a group first,
     * then each group's header and, unless it is folded, its connections.
     * Within each, the order they came in, which is most recently used.
     *
     * Unfolded, every group shows its connections whether it is folded or
     * not — what the columns are measured against, so folding one does not
     * shift them.
     */
    public function rows(bool $unfolded = false): array
    {
        $rows = [];

        $groups = $this->connections
            ->groupBy(fn (Connection $c) => trim((string) $c->group_name))
            ->sortKeysUsing(fn ($a, $b) => strcasecmp((string) $a, (string) $b));

        if ($groups->has('')) {
            foreach ($groups->pull('') as $connection) {
                $rows[] = $this->row($connection, '');
            }
        }

        foreach ($groups as $group => $connections) {
            // A group called 2024 comes back as an integer key.
            $group = (string) $group;
            $collapsed = ! $unfolded && $this->isCollapsed($group);

            $rows[] = [
                'header' => true,
                'group' => $group,
                'count' => $connections->count(),
                'collapsed' => $collapsed,
            ];

            if (! $collapsed) {
                foreach ($connections as $connection) {
                    $rows[] = $this->row($connection, $group);
                }
            }
        }

        return $rows;
    }

    private function row(Connection $c, string $group): array
    {
        return [
            'header' => false,
            'group' => $group,
            'id' => $c->id,
            'name' => $c->name,
            'tag' => (string) $c->tag,
            'read_only' => (bool) $c->read_only,
            'driver' => $c->driver,
            'where' => $c->describe(),
            'used' => $c->last_used_at?->diffForHumans() ?? 'never',
        ];
    }

    public function isCollapsed(string $group): bool
    {
        return in_array($group, $this->collapsed, true);
    }

    /** The connection under the cursor, or null on a header. */
    private function current(): ?Connection
    {
        $row = $this->rows()[$this->index] ?? null;

        return $row === null || $row['header']
            ? null
            : $this->connections->firstWhere('id', $row['id']);
    }

    private function indexOf(?int $id): ?int
    {
        foreach ($this->rows() as $i => $row) {
            if (! $row['header'] && $row['id'] === $id) {
                return $i;
            }
        }

        return null;
    }

    private function headerIndex(string $group): int
    {
        foreach ($this->rows() as $i => $row) {
            if ($row['header'] && $row['group'] === $group) {
                return $i;
            }
        }

        return $this->index;
    }

    /**
     * Fold or unfold the group under the cursor. From inside an open group,
     * folding lands on its header, so the cursor is not left on a row that
     * just disappeared.
     */
    private function fold(?bool $shut = null): bool
    {
        $row = $this->rows()[$this->index] ?? null;

        if ($row === null || $row['group'] === '') {
            return true;
        }

        $group = $row['group'];

        // Unfolding only means something on the header; inside, it is open.
        if (! $row['header'] && $shut !== true) {
            return true;
        }

        $shut ??= ! $this->isCollapsed($group);

        $this->collapsed = $shut
            ? array_values(array_unique([...$this->collapsed, $group]))
            : array_values(array_diff($this->collapsed, [$group]));

        Setting::write(self::COLLAPSED, json_encode($this->collapsed));

        $this->index = $this->headerIndex($group);

        return true;
    }

    public function onKey(string $key): void
    {
        if ($event = Mouse::parse($key)) {
            if (Layout::mouse()) {
                $this->onMouse($event);
            }

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
            $key === 'y' => $this->yankDatabase(),
            $key === 'Y' => $this->yankConnectionString(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $this->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $this->move(1),
            in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $this->fold(true),
            in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l'], true) => $this->fold(false),
            $key === ' ' => $this->fold(),
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

        if ($target < 0 || $target >= count($this->rows())) {
            return true;
        }

        $this->index = $target;

        return $this->select();
    }

    private function markDelete(): bool
    {
        $connection = $this->current();

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

    private function yankDatabase(): bool
    {
        $connection = $this->current();

        if ($connection === null) {
            return true;
        }

        $database = (string) $connection->database;

        if ($database === '') {
            $this->status = $connection->name.' has no database set — it asks on connect';

            return true;
        }

        return $this->yanked($database, $database);
    }

    private function yankConnectionString(): bool
    {
        $connection = $this->current();

        if ($connection === null) {
            return true;
        }

        return $this->yanked($connection->connectionString(), 'connection string');
    }

    private function yanked(string $text, string $what): bool
    {
        $where = Clipboard::copy($text);

        $this->status = 'yanked '.$what.($where === 'terminal' ? ' to the terminal clipboard' : '');

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
        $this->index = max(0, min($this->index, count($this->rows()) - 1));

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
        $connection = $this->current();

        if ($connection !== null) {
            $this->form = new ConnectionForm($connection);
            $this->status = null;
        }

        return true;
    }

    private function handleFormKey(string $key): void
    {
        $form = $this->form;

        // A list of key files is open over the form, so it gets the keys.
        if ($form->picker !== null) {
            match (true) {
                $key === Key::ESCAPE => $form->closePicker(),
                $key === Key::ENTER => $form->chooseFile(),
                in_array($key, Picker::UP, true) => $form->picker->move(-1),
                in_array($key, Picker::DOWN, true) => $form->picker->move(1),
                default => $form->picker->type($key),
            };

            return;
        }

        if ($form->editing) {
            $this->handleFieldKey($form, $key);

            return;
        }

        // The driver is a fixed set, so it cycles rather than being typed.
        if ($form->choices($form->currentKey()) !== null) {
            match (true) {
                $key === Key::ESCAPE, $key === 'q' => $this->closeForm('nothing changed'),
                $key === self::SAVE => $this->saveForm(),
                in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->move(-1),
                in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->move(1),
                in_array($key, [Key::LEFT, Key::LEFT_ARROW, 'h'], true) => $form->cycleValue(-1),
                in_array($key, [Key::RIGHT, Key::RIGHT_ARROW, 'l', ' '], true), $key === Key::ENTER => $form->cycleValue(1),
                default => true,
            };

            return;
        }

        match (true) {
            $key === Key::ESCAPE, $key === 'q' => $this->closeForm('nothing changed'),
            $key === self::SAVE => $this->saveForm(),
            in_array($key, [Key::UP, Key::UP_ARROW, 'k'], true) => $form->move(-1),
            in_array($key, [Key::DOWN, Key::DOWN_ARROW, 'j'], true) => $form->move(1),
            $key === Key::ENTER, $key === 'i' => in_array($form->currentKey(), ConnectionForm::PICKED, true)
                ? $form->openFilePicker()
                : $form->start(),
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

        // Follow the connection: a new one, or one moved into another group.
        // A folded group opens rather than hiding what was just saved.
        $group = trim((string) $connection->group_name);

        if ($group !== '' && $this->isCollapsed($group)) {
            $this->collapsed = array_values(array_diff($this->collapsed, [$group]));
            Setting::write(self::COLLAPSED, json_encode($this->collapsed));
        }

        $this->index = $this->indexOf($connection->id) ?? 0;

        $this->closeForm(($creating ? 'added ' : 'saved ').$connection->name);
    }

    private function closeForm(string $status): void
    {
        $this->form = null;
        $this->status = $status;
    }

    private function select(): bool
    {
        if (($this->rows()[$this->index]['header'] ?? false) === true) {
            return $this->fold();
        }

        $connection = $this->current();

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
        $this->index = max(0, min(count($this->rows()) - 1, $this->index + $by));

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
