<?php

namespace App\Prompts\Renderers;

use App\Tui\ConnectionPicker;
use Laravel\Prompts\Themes\Default\Renderer;

class ConnectionPickerRenderer extends Renderer
{
    public function __invoke(ConnectionPicker $prompt): string
    {
        $width = max(60, $prompt->terminal()->cols());
        $height = max(10, $prompt->terminal()->lines());

        $inner = $width - 4;
        $bodyHeight = max(3, $height - 7);

        $body = $this->body($prompt, $inner, $bodyHeight);

        $this->line($this->topBorder($inner));

        for ($i = 0; $i < $bodyHeight; $i++) {
            $this->line(
                $this->dim('│').' '.$this->pad($body[$i] ?? '', $inner).' '.$this->dim('│')
            );
        }

        $this->line($this->dim('└'.str_repeat('─', $inner + 2).'┘'));
        $this->line($this->status($prompt));
        $this->line($this->dim(' ↑↓ Move    ↵ Open    n New connection    :q Quit'));

        return $this;
    }

    private function topBorder(int $inner): string
    {
        $label = $this->bold(' CONNECTIONS ');
        $used = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $label));

        return $this->dim('┌─').$label.$this->dim(str_repeat('─', max(0, $inner + 2 - $used - 2))).$this->dim('┐');
    }

    private function status(ConnectionPicker $prompt): string
    {
        if ($prompt->command !== null) {
            return ' :'.$prompt->command.'█';
        }

        return $this->dim(' '.($prompt->status ?? $prompt->connections->count().' connections'));
    }

    private function body(ConnectionPicker $prompt, int $inner, int $height): array
    {
        $rows = $prompt->rows();

        if ($rows === []) {
            return ['', $this->dim('  No connections yet — press n to add one.')];
        }

        $nameWidth = min(28, max(4, max(array_map(fn ($r) => mb_strlen($r['name']), $rows))));
        $driverWidth = 8;
        $usedWidth = 18;
        $whereWidth = max(10, $inner - $nameWidth - $driverWidth - $usedWidth - 8);

        $lines = [
            $this->dim(
                '  '.$this->pad('NAME', $nameWidth).'  '.
                $this->pad('DRIVER', $driverWidth).'  '.
                $this->pad('WHERE', $whereWidth).'  '.
                $this->pad('LAST USED', $usedWidth)
            ),
            $this->dim('  '.str_repeat('─', min($inner - 2, $nameWidth + $driverWidth + $whereWidth + $usedWidth + 6))),
        ];

        $room = max(1, $height - count($lines));
        $start = count($rows) <= $room ? 0 : max(0, min($prompt->index - intdiv($room, 2), count($rows) - $room));

        $prompt->start = $start;

        foreach (array_slice($rows, $start, $room) as $offset => $row) {
            $text =
                '  '.$this->pad($this->truncate($row['name'], $nameWidth), $nameWidth).'  '.
                $this->pad($this->truncate($row['driver'], $driverWidth), $driverWidth).'  '.
                $this->pad($this->truncate($row['where'], $whereWidth), $whereWidth).'  '.
                $this->pad($this->truncate($row['used'], $usedWidth), $usedWidth);

            $lines[] = ($start + $offset) === $prompt->index
                ? $this->inverse($this->pad($text, $inner))
                : $text;
        }

        return $lines;
    }

    private function pad(string $text, int $width): string
    {
        $length = mb_strlen(preg_replace('/\e\[[0-9;]*m/', '', $text));

        return $length > $width ? $text : $text.str_repeat(' ', $width - $length);
    }
}
