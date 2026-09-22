<?php

namespace App\Tui\Concerns;

trait RendersWithoutPadding
{
    public function __toString()
    {
        return $this->output.(in_array($this->prompt->state, ['submit', 'cancel']) ? PHP_EOL : '');
    }
}
