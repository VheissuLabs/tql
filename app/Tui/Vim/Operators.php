<?php

namespace App\Tui\Vim;

use App\Tui\QueryEditor;

final class Operators
{
    public function __construct(private Vim $vim) {}

    public function apply(QueryEditor $editor, string $operator, Range $range): void
    {
        $range->linewise
            ? $this->applyToLines($editor, $operator, $range)
            : $this->applyToCharacters($editor, $operator, $range);
    }

    public function joinLines(QueryEditor $editor, int $firstLine, int $lastLine): void
    {
        $lastLine = max($lastLine, $firstLine + 1);
        $joinedAt = null;

        for ($line = $firstLine; $line < $lastLine; $line++) {
            $text = Text::of($editor->buffer());

            if ($firstLine >= $text->lineCount() - 1) {
                break;
            }

            $end = $text->lineEnd($text->startOfLine($firstLine));
            $next = $end + 1;

            while (in_array($text->at($next), [' ', "\t"], true)) {
                $next++;
            }

            $glue = $next >= $text->length || $text->at($next) === "\n" || $text->at($next) === ')' || $end === $text->lineStart($end)
                ? ''
                : ' ';

            $editor->replace($end, $next, $glue);
            $joinedAt = $end;
        }

        if ($joinedAt !== null) {
            $editor->moveTo($joinedAt);
        }
    }

    private function applyToCharacters(QueryEditor $editor, string $operator, Range $range): void
    {
        $text = Text::of($editor->buffer());
        $taken = $text->slice($range->start, $range->end);

        match ($operator) {
            'd' => $this->delete($editor, $range, $taken),
            'c' => $this->change($editor, $range, $taken),
            'y' => $this->yank($editor, $taken, false, $range->start),
            '~' => $this->swapCase($editor, $range, $taken),
            '>', '<', 'J' => $this->applyToLines($editor, $operator, new Range($range->start, max($range->start, $range->end - 1), true)),
            default => null,
        };
    }

    private function applyToLines(QueryEditor $editor, string $operator, Range $range): void
    {
        $text = Text::of($editor->buffer());
        $first = $text->lineOf($range->start);
        $last = $text->lineOf($range->end);
        $start = $text->startOfLine($first);
        $end = $text->lineEnd($text->startOfLine($last));
        $taken = $text->slice($start, $end)."\n";

        match ($operator) {
            'd' => $this->deleteLines($editor, $text, $start, $end, $taken),
            'c' => $this->changeLines($editor, $start, $end, $taken),
            'y' => $this->yankLines($editor, $text, $first, $taken),
            '>' => $this->indent($editor, $first, $last, fn (string $line) => $line === '' ? '' : '  '.$line),
            '<' => $this->indent($editor, $first, $last, fn (string $line) => (string) preg_replace('/^ {1,2}/', '', $line)),
            '~' => $this->swapCase($editor, new Range($start, $end), $text->slice($start, $end)),
            'J' => $this->joinLines($editor, $first, $last),
            default => null,
        };
    }

    private function delete(QueryEditor $editor, Range $range, string $taken): void
    {
        $this->vim->store($taken, false);
        $editor->replace($range->start, $range->end, '');
    }

    private function change(QueryEditor $editor, Range $range, string $taken): void
    {
        $this->delete($editor, $range, $taken);
        $this->vim->enterInsert($editor);
    }

    private function yank(QueryEditor $editor, string $taken, bool $linewise, int $cursor): void
    {
        $this->vim->store($taken, $linewise);
        $this->vim->yanked($taken);
        $editor->moveTo($cursor);
    }

    private function deleteLines(QueryEditor $editor, Text $text, int $start, int $end, string $taken): void
    {
        $this->vim->store($taken, true);

        [$from, $to] = match (true) {
            $end < $text->length => [$start, $end + 1],
            $start > 0 => [$start - 1, $end],
            default => [$start, $end],
        };

        $editor->replace($from, $to, '');

        $after = Text::of($editor->buffer());
        $editor->moveTo(Motions::firstNonBlank($after, $from === $start ? $start : $after->lineStart($from)));
    }

    private function changeLines(QueryEditor $editor, int $start, int $end, string $taken): void
    {
        $this->vim->store($taken, true);
        $editor->moveTo($start);

        $indent = $editor->autoindents()
            ? $editor->indentation()
            : '';

        $editor->replace($start, $end, $indent);
        $this->vim->enterInsert($editor);
    }

    private function yankLines(QueryEditor $editor, Text $text, int $first, string $taken): void
    {
        $column = $editor->cursorColumn();
        $cursor = $text->lineOf($editor->cursor()) === $first
            ? $editor->cursor()
            : $text->startOfLine($first) + min($column, max(0, $text->lineEnd($text->startOfLine($first)) - $text->startOfLine($first) - 1));

        $this->yank($editor, $taken, true, $cursor);
    }

    private function indent(QueryEditor $editor, int $first, int $last, callable $reshape): void
    {
        $lines = $editor->lines();

        for ($line = $first; $line <= $last; $line++) {
            $lines[$line] = $reshape($lines[$line]);
        }

        $editor->set(implode("\n", $lines));

        $text = Text::of($editor->buffer());
        $editor->moveTo(Motions::firstNonBlank($text, $text->startOfLine($first)));
    }

    private function swapCase(QueryEditor $editor, Range $range, string $taken): void
    {
        $swapped = implode('', array_map(
            fn (string $char) => mb_strtoupper($char) === $char ? mb_strtolower($char) : mb_strtoupper($char),
            mb_str_split($taken),
        ));

        $editor->replace($range->start, $range->end, $swapped);
        $editor->moveTo($range->start);
    }
}
