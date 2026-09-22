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

        $editing = $this->form->editor !== null && $here && $this->form->cell === FilterForm::VALUE;

        $room = max(4, $width - 44);

        // Fit the plain text first. Truncating afterwards would cut through
        // the cursor's escape sequence and the terminal draws the pieces.
        $value = match (true) {
            ! $filter->needsValue() => '',
            $editing => $this->typing($room),
            $filter->value === '' => '…',
            default => $this->style->truncate($filter->value, $room),
        };

        return $this->style->dim($joiner)
            .$this->cell($filter->column, $here && $this->form->cell === FilterForm::COLUMN, 18)
            .' '
            .$this->cell($filter->operator, $here && $this->form->cell === FilterForm::OPERATOR, 16)
            .' '
            // While typing, the cursor marks the spot; highlighting the cell
            // as well would wrap the cursor's own inverse in another one.
            .$this->cell($value, $here && $this->form->cell === FilterForm::VALUE && ! $editing, $room);
    }

    /**
     * The value being typed, windowed to the room available and with the
     * cursor drawn last so nothing cuts through it.
     */
    private function typing(int $room): string
    {
        $text = $this->form->editor->buffer();
        $cursor = $this->form->editor->cursor();

        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        // Scroll the value so the cursor stays in view.
        $start = max(0, $cursor - $room + 1);

        return $this->withCursor(
            implode('', array_slice($chars, $start, $room)),
            $cursor - $start,
        );
    }

    private function cell(string $text, bool $focused, int $width): string
    {
        $shown = $this->style->pad($text, $width);

        return $focused ? $this->style->color('selection', $shown) : $this->style->dim($shown);
    }

    private function withCursor(string $text, int $at): string
    {
        $chars = preg_split('//u', $text, -1, PREG_SPLIT_NO_EMPTY) ?: [];

        if ($at >= count($chars)) {
            return $text.$this->style->color('cursor', ' ');
        }

        $chars[$at] = $this->style->color('cursor', $chars[$at]);

        return implode('', $chars);
    }

    private function hint(): string
    {
        if ($this->form->editor !== null) {
            return '↵ keeps it    ↵↵ applies    esc goes back to the form';
        }

        return match ($this->form->cell) {
            FilterForm::VALUE => '↵ applies    type to edit    ⇥ ⇧⇥ moves    + adds    - removes',
            default => '← → changes it    ⇥ ⇧⇥ moves    ↵ opens a list    ctrl+s applies',
        };
    }
}
