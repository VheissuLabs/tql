<?php

namespace App\Tui\Islands;

use App\Tui\RecordForm;

class RecordFormIsland extends Island
{
    public const WIDTH = 76;

    private const FOOTER = 3;

    public function __construct(private RecordForm $form, private Styler $style)
    {
        $this->title = ($form->adds() ? 'NEW ROW' : 'EDIT ROW').'  ·  '.$form->table
            .($form->adds() ? '' : '  ·  '.$form->key.' '.$form->keyValue);
    }

    public function naturalWidth(): int
    {
        $longest = 0;

        foreach ($this->form->fields() as $field) {
            $value = $this->form->value($field['name']) ?? 'NULL';
            $shown = str_contains($value, "\n") ? 0 : mb_strlen($value);

            $longest = max($longest, mb_strlen($this->form->label($field)) + max($shown, mb_strlen($field['hint'])) + 1);
        }

        return max(self::WIDTH, $longest + 8);
    }

    public function rows(): int
    {
        return count($this->form->fields()) * 2 + self::FOOTER + 2;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $this->form->width = $innerWidth - 2;

        $body = [''];

        foreach ($this->form->fields() as $index => $field) {
            $body[] = $this->field($index, $field);
            $body[] = '';
        }

        $room = max(1, $innerHeight - self::FOOTER);
        $start = $this->window(1 + $this->form->index * 2, count($body), $room);

        return [
            ...array_slice($body, $start, $room),
            ...array_fill(0, max(0, $room - count($body)), ''),
            ...$this->footer(),
        ];
    }

    private function field(int $index, array $field): string
    {
        $here = $index === $this->form->index;
        $name = $field['name'];

        $label = $this->style->bold($name).$this->style->dim(' ('.$field['type'].'):').' ';
        $room = $this->form->room($field);

        if ($here && $this->form->editor !== null && ! $this->form->expanded) {
            return '  '.$label.$this->typing($room);
        }

        $shown = match ($this->form->state($name)) {
            RecordForm::UNTOUCHED => $this->style->truncate($field['hint'], $room),
            RecordForm::NULL => 'NULL',
            default => $this->clamp((string) $this->form->value($name), $room),
        };

        if ($here) {
            return '  '.$label.$this->style->color('cursor', $this->style->pad($shown, $room));
        }

        return '  '.$label.($this->form->state($name) === RecordForm::UNTOUCHED
            ? $this->style->dim($shown)
            : $this->paint($name, $shown));
    }

    private function paint(string $name, string $text): string
    {
        return $this->form->changed($name) ? $this->style->color('changed', $text) : $text;
    }

    private function clamp(string $value, int $room): string
    {
        $line = str_replace("\n", ' ', $value);

        if (mb_strlen($line) <= $room && ! str_contains($value, "\n")) {
            return $line;
        }

        return mb_substr($line, 0, max(1, $room - 3)).'… ↗';
    }

    private function typing(int $room): string
    {
        $editor = $this->form->editor;
        $text = $editor->buffer();
        $cursor = $editor->cursor();

        $start = max(0, $cursor - $room + 1);
        $visible = mb_substr($text, $start, $room);
        $at = $cursor - $start;

        return mb_substr($visible, 0, $at)
            .$this->style->color('cursor', mb_substr($visible, $at, 1) ?: ' ')
            .mb_substr($visible, $at + 1);
    }

    private function footer(): array
    {
        $hint = match (true) {
            $this->form->editor !== null => '↵ keep  tab next  ctrl+t now  esc put it back',
            $this->form->discarding => 'esc again throws the row away  any other key keeps it open',
            default => '↑↓ move  ↵ edit  ctrl+n null  ⌫ reset  ctrl+s keep  esc cancel',
        };

        $room = $this->form->width - 2;

        return [
            '',
            $this->form->error === null ? '' : '  '.$this->style->color('problem', $this->style->truncate($this->form->error, $room)),
            '  '.$this->style->dim($this->style->truncate($hint, $room)),
        ];
    }
}
