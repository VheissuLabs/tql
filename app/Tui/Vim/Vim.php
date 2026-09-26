<?php

namespace App\Tui\Vim;

use App\Tui\Clipboard;
use App\Tui\QueryEditor;
use Laravel\Prompts\Key;

final class Vim
{
    public const NOTHING = 'nothing';

    public const LEAVE = 'leave';

    public const NEXT_PANE = 'next_pane';

    public const PREVIOUS_PANE = 'previous_pane';

    public const COMMAND = 'command';

    public const REDO = "\x12";

    private const OPERATORS = ['d', 'c', 'y', '>', '<'];

    private const FINDS = ['f', 't', 'F', 'T'];

    private const MOTIONS = [
        'h', 'l', 'j', 'k', '0', '^', '$', 'w', 'b', 'e', 'W', 'B', 'E', 'G', ';', ',', '%', '{', '}', ' ',
        Key::ENTER, Key::LEFT, Key::LEFT_ARROW, Key::RIGHT, Key::RIGHT_ARROW, Key::UP, Key::UP_ARROW, Key::DOWN, Key::DOWN_ARROW,
        Key::BACKSPACE,
    ];

    private const OBJECTS = ['w', 'W', '(', ')', 'b', '[', ']', '{', '}', 'B', '"', "'", '`', 'p'];

    private const WAITS_FOR_CHARACTER = ['r'];

    private const CHANGES = ['x', 'X', 'D', 'C', 's', 'S', 'r', 'J', '~', 'p', 'P', 'i', 'a', 'I', 'A', 'o', 'O'];

    public string $mode = 'normal';

    public string $register = '';

    public bool $registerIsLines = false;

    public ?string $message = null;

    private array $keys = [];

    private ?int $anchor = null;

    private ?int $column = null;

    private ?array $lastFind = null;

    private ?array $lastChange = null;

    private ?array $recording = null;

    private bool $replaying = false;

    private Operators $operators;

    public function __construct()
    {
        $this->operators = new Operators($this);
    }

    public function reset(): void
    {
        $this->mode = 'normal';
        $this->keys = [];
        $this->anchor = null;
        $this->recording = null;
    }

    public function press(QueryEditor $editor, string $key): string
    {
        $this->message = null;

        if ($this->mode === 'insert') {
            $this->pressInInsert($editor, $key);

            return self::NOTHING;
        }

        $outcome = $this->mode === 'normal'
            ? $this->pressInNormal($editor, $key)
            : $this->pressInVisual($editor, $key);

        if ($this->mode !== 'insert') {
            $this->keepOnCharacter($editor);
        }

        return $outcome;
    }

    public function selection(QueryEditor $editor): ?Range
    {
        if ($this->anchor === null) {
            return null;
        }

        $text = Text::of($editor->buffer());
        $from = min($this->anchor, $editor->cursor());
        $to = max($this->anchor, $editor->cursor());

        if ($this->mode === 'visual line') {
            return new Range($text->lineStart($from), $text->lineEnd($to), true);
        }

        return new Range($from, min($text->length, $to + 1));
    }

    public function selectedText(QueryEditor $editor): ?string
    {
        $range = $this->selection($editor);

        return $range === null
            ? null
            : Text::of($editor->buffer())->slice($range->start, $range->end);
    }

    public function leaveVisual(): void
    {
        if ($this->anchor !== null) {
            $this->mode = 'normal';
            $this->anchor = null;
        }
    }

    public function store(string $text, bool $lines): void
    {
        $this->register = $text;
        $this->registerIsLines = $lines;
    }

    public function yanked(string $text): void
    {
        Clipboard::copy($text);

        $lines = substr_count(rtrim($text, "\n"), "\n") + 1;

        $this->message = $this->registerIsLines
            ? "yanked {$lines} ".($lines === 1 ? 'line' : 'lines')
            : 'yanked '.mb_strlen($text).' '.(mb_strlen($text) === 1 ? 'character' : 'characters');
    }

    public function enterInsert(QueryEditor $editor): void
    {
        $editor->checkpoint();
        $this->mode = 'insert';
        $this->anchor = null;
    }

    private function pressInInsert(QueryEditor $editor, string $key): void
    {
        if ($this->recording !== null) {
            $this->recording[] = $key;
        }

        if ($key === Key::ESCAPE) {
            $this->leaveInsert($editor);

            return;
        }

        if (in_array($key, [Key::LEFT, Key::LEFT_ARROW, Key::RIGHT, Key::RIGHT_ARROW, Key::UP, Key::UP_ARROW, Key::DOWN, Key::DOWN_ARROW], true)) {
            $editor->commit();
            $editor->handle($key);
            $editor->checkpoint();

            return;
        }

        match ($key) {
            "\x17" => $this->deleteWordBefore($editor),
            "\x15" => $this->deleteToLineStart($editor),
            self::REDO => null,
            "\e[13;2u", "\e\r", "\e\n" => $editor->handle(Key::ENTER),
            default => $editor->handle($key),
        };
    }

    private function leaveInsert(QueryEditor $editor): void
    {
        $editor->commit();
        $this->mode = 'normal';

        $text = Text::of($editor->buffer());

        if ($editor->cursor() > $text->lineStart($editor->cursor())) {
            $editor->moveTo($editor->cursor() - 1);
        }

        if ($this->recording !== null && ! $this->replaying) {
            $this->lastChange = $this->recording;
        }

        $this->recording = null;
    }

    private function pressInNormal(QueryEditor $editor, string $key): string
    {
        if ($this->keys === [] && in_array($key, [Key::ESCAPE, Key::TAB, Key::SHIFT_TAB, ':'], true)) {
            return match ($key) {
                Key::ESCAPE => self::LEAVE,
                Key::TAB => self::NEXT_PANE,
                Key::SHIFT_TAB => self::PREVIOUS_PANE,
                ':' => self::COMMAND,
            };
        }

        if ($key === Key::ESCAPE) {
            $this->keys = [];

            return self::NOTHING;
        }

        $this->keys[] = $key;
        $command = $this->parse($this->keys, allowOperators: true);

        if ($command === null) {
            return self::NOTHING;
        }

        $this->keys = [];

        if ($command === false) {
            return self::NOTHING;
        }

        $this->run($editor, $command);

        return self::NOTHING;
    }

    private function pressInVisual(QueryEditor $editor, string $key): string
    {
        if ($this->keys === []) {
            $handled = $this->visualKey($editor, $key);

            if ($handled !== null) {
                return $handled;
            }
        }

        if ($key === Key::ESCAPE) {
            $this->keys = [];

            return self::NOTHING;
        }

        $this->keys[] = $key;
        $command = $this->parse($this->keys, allowOperators: false, allowObjects: true);

        if ($command === null) {
            return self::NOTHING;
        }

        $this->keys = [];

        if ($command === false) {
            return self::NOTHING;
        }

        if (isset($command['object'])) {
            $range = $this->resolveObject($editor, $command['object']);

            if ($range !== null && $range->end > $range->start) {
                $this->anchor = $range->start;
                $editor->moveTo($range->end - 1);
            }

            return self::NOTHING;
        }

        if (isset($command['motion'])) {
            $this->move($editor, $command['motion'], $command['count'], $command['counted']);
        }

        return self::NOTHING;
    }

    private function visualKey(QueryEditor $editor, string $key): ?string
    {
        $selection = $this->selection($editor);

        return match ($key) {
            Key::ESCAPE => $this->then(fn () => $this->leaveVisual()),
            ':' => self::COMMAND,
            'o' => $this->then(function () use ($editor) {
                $cursor = $editor->cursor();
                $editor->moveTo((int) $this->anchor);
                $this->anchor = $cursor;
            }),
            'v', 'V' => $this->then(fn () => $this->switchVisual($key === 'v' ? 'visual' : 'visual line')),
            'd', 'x', 'y', 'c', '>', '<', '~', 'J' => $this->then(fn () => $this->operateOnSelection($editor, $key === 'x' ? 'd' : $key, $selection)),
            default => null,
        };
    }

    private function switchVisual(string $mode): void
    {
        if ($this->mode === $mode) {
            $this->leaveVisual();

            return;
        }

        $this->mode = $mode;
    }

    private function operateOnSelection(QueryEditor $editor, string $operator, Range $selection): void
    {
        $this->anchor = null;
        $this->mode = 'normal';

        $editor->checkpoint();
        $this->operators->apply($editor, $operator, $selection);

        if ($this->mode !== 'insert') {
            $editor->commit();
        }
    }

    private function then(callable $action): string
    {
        $action();

        return self::NOTHING;
    }

    private function parse(array $keys, bool $allowOperators, bool $allowObjects = false): array|false|null
    {
        $at = 0;
        $first = $this->readCount($keys, $at);

        if (! isset($keys[$at])) {
            return null;
        }

        $key = $keys[$at];

        if ($allowOperators && in_array($key, self::OPERATORS, true)) {
            return $this->parseOperator($keys, $at + 1, $key, $first);
        }

        $target = $this->parseTarget($keys, $at, $allowObjects);

        if ($target !== false) {
            return $target === null
                ? null
                : $target + ['count' => $first ?? 1, 'counted' => $first !== null, 'body' => array_slice($keys, $at)];
        }

        if (! $allowOperators) {
            return false;
        }

        if (in_array($key, self::WAITS_FOR_CHARACTER, true) && ! isset($keys[$at + 1])) {
            return null;
        }

        return [
            'action' => $key,
            'character' => $keys[$at + 1] ?? null,
            'count' => $first ?? 1,
            'counted' => $first !== null,
            'body' => array_slice($keys, $at),
        ];
    }

    private function parseOperator(array $keys, int $at, string $operator, ?int $first): array|false|null
    {
        if (! isset($keys[$at])) {
            return null;
        }

        $body = [$operator];

        if ($keys[$at] === $operator) {
            return ['operator' => $operator, 'lines' => true, 'count' => $first ?? 1, 'counted' => $first !== null, 'body' => [$operator, $operator]];
        }

        $second = $this->readCount($keys, $at);

        if (! isset($keys[$at])) {
            return null;
        }

        $target = $this->parseTarget($keys, $at, allowObjects: true);

        if ($target === null || $target === false) {
            return $target;
        }

        $counted = $first !== null || $second !== null;

        return $target + [
            'operator' => $operator,
            'count' => ($first ?? 1) * ($second ?? 1),
            'counted' => $counted,
            'body' => array_merge($body, array_slice($keys, $at)),
        ];
    }

    private function parseTarget(array $keys, int $at, bool $allowObjects): array|false|null
    {
        $key = $keys[$at];
        $next = $keys[$at + 1] ?? null;

        if (in_array($key, self::FINDS, true)) {
            return $next === null
                ? null
                : ['motion' => [$key, $next]];
        }

        if ($key === 'g') {
            return match ($next) {
                null => null,
                'g' => ['motion' => ['gg']],
                default => false,
            };
        }

        if ($allowObjects && in_array($key, ['i', 'a'], true)) {
            if ($next === null) {
                return null;
            }

            return in_array($next, self::OBJECTS, true)
                ? ['object' => [$key === 'a', $next]]
                : false;
        }

        if (in_array($key, self::MOTIONS, true)) {
            return ['motion' => [$key]];
        }

        return false;
    }

    private function readCount(array $keys, int &$at): ?int
    {
        $digits = '';

        while (isset($keys[$at]) && ctype_digit($keys[$at]) && ($digits !== '' || $keys[$at] !== '0')) {
            $digits .= $keys[$at++];
        }

        return $digits === ''
            ? null
            : (int) $digits;
    }

    private function run(QueryEditor $editor, array $command): void
    {
        if (isset($command['operator'])) {
            $this->operate($editor, $command);
            $this->remember($command);

            return;
        }

        if (isset($command['motion'])) {
            $this->move($editor, $command['motion'], $command['count'], $command['counted']);

            return;
        }

        $this->act($editor, $command);

        if (in_array($command['action'], self::CHANGES, true)) {
            $this->remember($command);
        }
    }

    private function remember(array $command): void
    {
        if ($this->replaying || ($command['operator'] ?? null) === 'y') {
            return;
        }

        $keys = array_merge($command['counted'] ? str_split((string) $command['count']) : [], $command['body']);

        if ($this->mode === 'insert') {
            $this->recording = $keys;

            return;
        }

        $this->lastChange = $keys;
    }

    private function move(QueryEditor $editor, array $motion, int $count, bool $counted): void
    {
        $text = Text::of($editor->buffer());
        $resolved = $this->resolveMotion($text, $editor->cursor(), $motion, $count, $counted);

        if ($resolved !== null) {
            $editor->moveTo($resolved[0]);
        }
    }

    private function operate(QueryEditor $editor, array $command): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();
        $operator = $command['operator'];

        $range = match (true) {
            isset($command['lines']) => $this->currentLines($text, $cursor, $command['count']),
            isset($command['object']) => $this->resolveObject($editor, $command['object']),
            default => $this->motionRange($text, $cursor, $operator, $command['motion'], $command['count'], $command['counted']),
        };

        if ($range === null) {
            return;
        }

        $editor->checkpoint();
        $this->operators->apply($editor, $operator, $range);

        if ($this->mode !== 'insert') {
            $editor->commit();
        }
    }

    private function currentLines(Text $text, int $cursor, int $count): Range
    {
        $line = $text->lineOf($cursor);

        return new Range(
            $text->startOfLine($line),
            $text->startOfLine(min($text->lineCount() - 1, $line + $count - 1)),
            true,
        );
    }

    private function motionRange(Text $text, int $cursor, string $operator, array $motion, int $count, bool $counted): ?Range
    {
        if ($operator === 'c' && in_array($motion[0], ['w', 'W'], true) && Motions::kind($text->at($cursor)) !== 0) {
            $motion = [$motion[0] === 'w' ? 'e' : 'E'];
        }

        $resolved = $this->resolveMotion($text, $cursor, $motion, $count, $counted);

        if ($resolved === null) {
            return null;
        }

        [$target, $kind] = $resolved;

        if ($kind === 'linewise') {
            return Range::between($cursor, $target, true);
        }

        if ($kind === 'inclusive') {
            $last = max($cursor, $target);
            $end = $text->at($last) === "\n" || $last >= $text->length
                ? $last
                : $last + 1;

            return new Range(min($cursor, $target), $end);
        }

        if (in_array($motion[0], ['w', 'W'], true) && $target > $cursor) {
            $target = $this->endOfLastWord($text, $cursor, $target);
        }

        return Range::between($cursor, $target);
    }

    private function endOfLastWord(Text $text, int $cursor, int $target): int
    {
        if ($text->lineOf($target) === $text->lineOf($cursor) || $target >= $text->length) {
            return $target;
        }

        $end = $target;

        while ($end > $cursor && Motions::kind($text->at($end - 1)) === 0) {
            $end--;
        }

        return $end;
    }

    private function resolveMotion(Text $text, int $cursor, array $motion, int $count, bool $counted): ?array
    {
        $key = $motion[0];
        $keepsColumn = in_array($key, ['j', 'k', Key::UP, Key::UP_ARROW, Key::DOWN, Key::DOWN_ARROW], true);

        if (! $keepsColumn) {
            $this->column = null;
        }

        $column = $this->column ?? $cursor - $text->lineStart($cursor);

        $resolved = match ($key) {
            'h', Key::LEFT, Key::LEFT_ARROW, Key::BACKSPACE => [Motions::left($text, $cursor, $count), 'exclusive'],
            'l', ' ', Key::RIGHT, Key::RIGHT_ARROW => [Motions::right($text, $cursor, $count), 'exclusive'],
            'j', Key::DOWN, Key::DOWN_ARROW => $this->vertical($text, $cursor, $count, $column),
            'k', Key::UP, Key::UP_ARROW => $this->vertical($text, $cursor, -$count, $column),
            Key::ENTER => $this->nextLine($text, $cursor, $count),
            '0' => [Motions::lineStart($text, $cursor), 'exclusive'],
            '^' => [Motions::firstNonBlank($text, $cursor), 'exclusive'],
            '$' => [Motions::lastCharacter($text, $cursor, $count), 'inclusive'],
            'w', 'W' => [Motions::wordForward($text, $cursor, $count, $key === 'W'), 'exclusive'],
            'b', 'B' => [Motions::wordBackward($text, $cursor, $count, $key === 'B'), 'exclusive'],
            'e', 'E' => [Motions::wordEnd($text, $cursor, $count, $key === 'E'), 'inclusive'],
            'G' => [Motions::line($text, $counted ? $count - 1 : $text->lineCount() - 1), 'linewise'],
            'gg' => [Motions::line($text, $counted ? $count - 1 : 0), 'linewise'],
            'f', 't', 'F', 'T' => $this->find($text, $cursor, $key, $motion[1], $count),
            ';', ',' => $this->repeatFind($text, $cursor, $key === ',', $count),
            '%' => $this->bracket($text, $cursor),
            '}' => [Motions::paragraphForward($text, $cursor, $count), 'exclusive'],
            '{' => [Motions::paragraphBackward($text, $cursor, $count), 'exclusive'],
            default => null,
        };

        if ($key === '$') {
            $this->column = PHP_INT_MAX;
        }

        return $resolved;
    }

    private function vertical(Text $text, int $cursor, int $by, int $column): ?array
    {
        $this->column = $column;
        $target = Motions::vertical($text, $cursor, $by, $column);

        return $target === null
            ? null
            : [$target, 'linewise'];
    }

    private function nextLine(Text $text, int $cursor, int $count): ?array
    {
        $target = Motions::vertical($text, $cursor, $count, 0);

        return $target === null
            ? null
            : [Motions::firstNonBlank($text, $target), 'linewise'];
    }

    private function find(Text $text, int $cursor, string $key, string $char, int $count): ?array
    {
        $this->lastFind = [$key, $char];

        return $this->findWith($text, $cursor, $key, $char, $count);
    }

    private function repeatFind(Text $text, int $cursor, bool $reverse, int $count): ?array
    {
        if ($this->lastFind === null) {
            return null;
        }

        [$key, $char] = $this->lastFind;

        if ($reverse) {
            $key = ['f' => 'F', 'F' => 'f', 't' => 'T', 'T' => 't'][$key];
        }

        return $this->findWith($text, $cursor, $key, $char, $count);
    }

    private function findWith(Text $text, int $cursor, string $key, string $char, int $count): ?array
    {
        $forward = in_array($key, ['f', 't'], true);
        $till = in_array($key, ['t', 'T'], true);
        $target = Motions::findInLine($text, $cursor, $char, $count, $forward, $till);

        if ($target === null) {
            return null;
        }

        return [$target, $forward ? 'inclusive' : 'exclusive'];
    }

    private function bracket(Text $text, int $cursor): ?array
    {
        $target = Motions::matchingBracket($text, $cursor);

        return $target === null
            ? null
            : [$target, 'inclusive'];
    }

    private function resolveObject(QueryEditor $editor, array $object): ?Range
    {
        [$around, $kind] = $object;
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();

        return match ($kind) {
            'w', 'W' => TextObjects::word($text, $cursor, $around, $kind === 'W'),
            '(', ')', 'b' => TextObjects::bracket($text, $cursor, '(', $around),
            '[', ']' => TextObjects::bracket($text, $cursor, '[', $around),
            '{', '}', 'B' => TextObjects::bracket($text, $cursor, '{', $around),
            '"', "'", '`' => TextObjects::quote($text, $cursor, $kind, $around),
            'p' => TextObjects::paragraph($text, $cursor, $around),
            default => null,
        };
    }

    private function act(QueryEditor $editor, array $command): void
    {
        $count = $command['count'];

        match ($command['action']) {
            'x' => $this->operateWith($editor, 'd', ['l'], $count),
            'X' => $this->operateWith($editor, 'd', ['h'], $count),
            'D' => $this->operateWith($editor, 'd', ['$'], $count),
            'C' => $this->operateWith($editor, 'c', ['$'], $count),
            's' => $this->substitute($editor, $count),
            'S' => $this->operate($editor, ['operator' => 'c', 'lines' => true, 'count' => $count]),
            'Y' => $this->operate($editor, ['operator' => 'y', 'lines' => true, 'count' => $count]),
            'r' => $this->replaceCharacters($editor, (string) $command['character'], $count),
            'J' => $this->join($editor, $count),
            '~' => $this->swapCase($editor, $count),
            'p', 'P' => $this->put($editor, $command['action'] === 'p', $count),
            'u' => $this->undo($editor, $count),
            self::REDO => $this->redo($editor, $count),
            '.' => $this->repeat($editor, $command['counted'] ? $count : null),
            'i', 'a', 'I', 'A', 'o', 'O' => $this->insert($editor, $command['action']),
            'v' => $this->startVisual($editor, 'visual'),
            'V' => $this->startVisual($editor, 'visual line'),
            default => null,
        };
    }

    private function operateWith(QueryEditor $editor, string $operator, array $motion, int $count): void
    {
        $text = Text::of($editor->buffer());

        if ($text->isBlankLine($editor->cursor()) && $motion !== ['$']) {
            return;
        }

        $this->operate($editor, ['operator' => $operator, 'motion' => $motion, 'count' => $count, 'counted' => true]);
    }

    private function substitute(QueryEditor $editor, int $count): void
    {
        $text = Text::of($editor->buffer());

        if ($text->isBlankLine($editor->cursor())) {
            $this->enterInsert($editor);

            return;
        }

        $this->operateWith($editor, 'c', ['l'], $count);
    }

    private function replaceCharacters(QueryEditor $editor, string $char, int $count): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();

        if (mb_strlen($char) !== 1 || ctype_cntrl($char) || $cursor + $count > $text->lineEnd($cursor)) {
            return;
        }

        $editor->checkpoint();
        $editor->replace($cursor, $cursor + $count, str_repeat($char, $count));
        $editor->moveTo($cursor + $count - 1);
        $editor->commit();
    }

    private function join(QueryEditor $editor, int $count): void
    {
        $text = Text::of($editor->buffer());
        $line = $text->lineOf($editor->cursor());

        $editor->checkpoint();
        $this->operators->joinLines($editor, $line, $line + max(1, $count - 1));
        $editor->commit();
    }

    private function swapCase(QueryEditor $editor, int $count): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();
        $end = min($text->lineEnd($cursor), $cursor + $count);

        if ($end <= $cursor) {
            return;
        }

        $editor->checkpoint();
        $this->operators->apply($editor, '~', new Range($cursor, $end));
        $editor->moveTo($end);
        $editor->commit();
    }

    private function put(QueryEditor $editor, bool $after, int $count): void
    {
        if ($this->register === '') {
            return;
        }

        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();
        $editor->checkpoint();

        if ($this->registerIsLines) {
            $lines = str_repeat($this->register, $count);

            if ($after) {
                $at = $text->lineEnd($cursor);
                $editor->replace($at, $at, "\n".rtrim($lines, "\n"));
                $editor->moveTo(Motions::firstNonBlank(Text::of($editor->buffer()), $at + 1));
            } else {
                $at = $text->lineStart($cursor);
                $editor->replace($at, $at, $lines);
                $editor->moveTo(Motions::firstNonBlank(Text::of($editor->buffer()), $at));
            }

            $editor->commit();

            return;
        }

        $at = $after && ! $text->isBlankLine($cursor)
            ? min($cursor + 1, $text->lineEnd($cursor))
            : $cursor;

        $pasted = str_repeat($this->register, $count);
        $editor->replace($at, $at, $pasted);
        $editor->moveTo($at + mb_strlen($pasted) - 1);
        $editor->commit();
    }

    private function undo(QueryEditor $editor, int $count): void
    {
        for ($step = 0; $step < $count; $step++) {
            if (! $editor->undo()) {
                $this->message = 'already at oldest change';

                return;
            }
        }
    }

    private function redo(QueryEditor $editor, int $count): void
    {
        for ($step = 0; $step < $count; $step++) {
            if (! $editor->redo()) {
                $this->message = 'already at newest change';

                return;
            }
        }
    }

    private function repeat(QueryEditor $editor, ?int $count): void
    {
        if ($this->lastChange === null) {
            return;
        }

        $keys = $this->lastChange;

        if ($count !== null) {
            while ($keys !== [] && ctype_digit($keys[0])) {
                array_shift($keys);
            }

            $keys = array_merge(str_split((string) $count), $keys);
        }

        $this->replaying = true;

        foreach ($keys as $key) {
            $this->press($editor, $key);
        }

        $this->replaying = false;
    }

    private function insert(QueryEditor $editor, string $key): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();

        $this->enterInsert($editor);

        match ($key) {
            'a' => $editor->moveTo(min($cursor + 1, $text->lineEnd($cursor))),
            'I' => $editor->moveTo(Motions::firstNonBlank($text, $cursor)),
            'A' => $editor->moveTo($text->lineEnd($cursor)),
            'o' => $editor->openLine(),
            'O' => $editor->openLine(above: true),
            default => null,
        };
    }

    private function startVisual(QueryEditor $editor, string $mode): void
    {
        $this->mode = $mode;
        $this->anchor = $editor->cursor();
    }

    private function deleteWordBefore(QueryEditor $editor): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();
        $lineStart = $text->lineStart($cursor);

        if ($cursor === $lineStart) {
            $editor->handle(Key::BACKSPACE);

            return;
        }

        $start = $cursor;

        while ($start > $lineStart && Motions::kind($text->at($start - 1)) === 0) {
            $start--;
        }

        $kind = Motions::kind($text->at($start - 1));

        while ($start > $lineStart && Motions::kind($text->at($start - 1)) === $kind && $kind !== 0) {
            $start--;
        }

        $editor->replace($start, $cursor, '');
    }

    private function deleteToLineStart(QueryEditor $editor): void
    {
        $cursor = $editor->cursor();
        $editor->replace(Text::of($editor->buffer())->lineStart($cursor), $cursor, '');
    }

    private function keepOnCharacter(QueryEditor $editor): void
    {
        $text = Text::of($editor->buffer());
        $cursor = $editor->cursor();
        $end = $text->lineEnd($cursor);

        if ($cursor >= $end && $end > $text->lineStart($cursor)) {
            $editor->moveTo($end - 1);
        }
    }
}
