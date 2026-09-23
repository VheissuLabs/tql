<?php

namespace App\Commands;

use App\Support\ConfigFile;
use App\Support\ConfigTemplate;
use App\Support\ConfigTidy;
use App\Support\Paths;
use LaravelZero\Framework\Commands\Command;

class ConfigCommand extends Command
{
    protected $signature = 'config
        {--tidy : Put the settings back in order, keeping your values}
        {--path : Print the path to the file and stop}';

    protected $description = 'Show where the config file is, and put it back in order';

    protected $help = <<<'HELP'
    tql owns the shape of the file — which sections there are, and the order the
    settings come in. You own what is in it: the values, which settings you keep,
    and any comment you write. <fg=green>--tidy</> reorders without touching any of that,
    and writes a <fg=yellow>.bak</> beside it first.

    A setting you delete stays deleted: the file records the version it was
    written for, and only settings newer than that are ever added back.
    HELP;

    public function handle(): int
    {
        $file = Paths::configFile();

        if ($this->option('path')) {
            $this->line($file);

            return self::SUCCESS;
        }

        if (! file_exists($file)) {
            $this->error("No config file yet. Run tql once and it writes {$file}.");

            return self::FAILURE;
        }

        $contents = (string) file_get_contents($file);
        $moved = [];
        $tidied = ConfigTidy::apply($contents, $moved);

        if (! $this->option('tidy')) {
            return $this->report($file, $contents, $moved);
        }

        if ($tidied === $contents) {
            $this->line('  Already in order: <fg=cyan>'.$file.'</>');

            return self::SUCCESS;
        }

        copy($file, $file.'.bak');
        file_put_contents($file, $tidied);

        $this->line('  Tidied <fg=cyan>'.$file.'</>');
        $this->line('  '.count($moved).' setting'.(count($moved) === 1 ? '' : 's').
            ' moved: '.implode(', ', array_unique($moved)));
        $this->line('  The file as it was: <fg=yellow>'.$file.'.bak</>');

        return self::SUCCESS;
    }

    /**
     * @param  array<int, string>  $moved
     */
    private function report(string $file, string $contents, array $moved): int
    {
        $written = ConfigFile::writtenFor($contents) ?? 'before tql wrote versions into it';

        $this->line('  <fg=cyan>'.$file.'</>');
        $this->line('  written for '.$written.', template is '.ConfigTemplate::VERSION);

        if ($moved === []) {
            $this->line('  <fg=green>in order</>');

            return self::SUCCESS;
        }

        $this->line('  <fg=yellow>out of order</>: '.implode(', ', array_unique($moved)));
        $this->line('  <fg=green>tql config --tidy</> sorts it, keeping your values.');

        return self::SUCCESS;
    }
}
