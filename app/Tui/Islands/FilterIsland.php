<?php

namespace App\Tui\Islands;

use App\Tui\FilterForm;

class FilterIsland extends Island
{
    public const WIDTH = 72;

    public string $title = 'FILTER';

    public function __construct(
        private FilterForm $form,
        private Styler $style,
    ) {}

    public function rows(): int
    {
        return count($this->form->conditions) + 5;
    }

    public function content(int $innerWidth, int $innerHeight): array
    {
        $lines = [''];

        foreach ($this->form->conditions as $index => $filter) {
            $lines[] = '  '.$this->condition($index, $innerWidth - 4);
        }

        $lines[] = '';
        $lines[] = '  '.$this->style->dim($this->hint());

        return array_slice($lines, 0, $innerHeight);
    }

    private function condition(int $index, int $width): string
    {
        $filter = $this->form->conditions[$index];
        $here = $index === $this->form->row;

        // Rows after the first are joined by and/or, which is the one thing
        // you cannot see from the condition itself.
        $joiner = $index === 0
            ? str_pad('where', 6)
            : str_pad($this->form->joiner, 6);

        $value = $filter->needsValue()
            ? ($this->form->editor !== null && $here && $this->form->cell === FilterForm::VALUE
                ? $this->withCursor($this->form->editor->buffer(), $this->form->editor->cursor())
                : ($filter->value === '' ? '…' : $filter->value))
            : '';

        return $this->style->dim($joiner)
            .$this->cell($filter->column, $here && $this->form->cell === FilterForm::COLUMN, 18)
            .' '
            .$this->cell($filter->operator, $here && $this->form->cell === FilterForm::OPERATOR, 16)
            .' '
            .$this->cell($value, $here && $this->form->cell === FilterForm::VALUE, max(4, $width - 44));
    }

    private function cell(string $text, bool $focused, int $width): string
    {
        $shown = $this->style->pad($this->style->truncate($text, $width), $width);

        return $focused ? $this->style->colour('selection', $shown) : $this->style->dim($shown);
    }

    private function withCursor(string $text, int $at): string
    {
        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        if ($at >= count($chars)) {
            return $text.$this->style->colour('cursor', ' ');
        }

        $chars[$at] = $this->style->colour('cursor', $chars[$at]);

        return implode('', $chars);
    }

    private function hint(): string
    {
        if ($this->form->editor !== null) {
            return '↵ keeps it    esc drops it';
        }

        return match ($this->form->cell) {
            FilterForm::VALUE => '↵ types a value    + adds    - removes    ctrl+s applies    esc clears',
            default => '← → changes it    ↑↓ moves    + adds    - removes    ctrl+s applies',
        };
    }
}
