<?php

namespace App\Commands;

use App\Support\Paths;
use App\TablePlus\Importer;
use LaravelZero\Framework\Commands\Command;
use Throwable;

class TablePlusCommand extends Command
{
    protected $signature = 'tableplus
        {--dry-run : List what would come over, and touch nothing}
        {--without-passwords : Leave passwords in the Keychain}
        {--from= : The TablePlus Data folder, when it is not in the usual place}
        {--json : JSON, which is what you get when the output is piped}
        {--table : Lines for a person, which is what you get in a terminal}';

    protected $description = 'Bring your TablePlus connections into tql, groups and passwords too';

    protected $help = <<<'HELP'
    Reads the connections TablePlus has saved on this Mac and adds each one to
    tql, with its group, its environment as a tag, its SSH tunnel and TLS
    settings. Passwords come from the Keychain, and macOS asks you to allow
    each one. A connection tql already has is left alone, so running it twice
    is safe.

    <fg=yellow>For an agent</>

      Run <fg=green>tql tableplus --dry-run</> first and show the person what will
      come over, then <fg=green>tql tableplus</>. Both answer in JSON when piped.
      Tell the person to watch for the macOS Keychain prompts.
    HELP;

    public function handle(Importer $importer): int
    {
        $directory = $this->option('from') === null
            ? Importer::find()
            : rtrim(Paths::resolve($this->option('from')), '/');

        if ($directory === null) {
            return $this->refuse(PHP_OS_FAMILY === 'Darwin'
                ? 'TablePlus has no saved connections on this Mac.'
                : 'tql reads TablePlus for Mac; TablePlus for Windows and Linux is not supported yet.');
        }

        try {
            $plan = $importer->plan($directory);

            $results = $this->option('dry-run')
                ? $plan
                : $importer->import($plan, ! $this->option('without-passwords'));
        } catch (Throwable $exception) {
            return $this->refuse($exception->getMessage());
        }

        $this->wantsJson()
            ? $this->line(json_encode($this->report($directory, $results), JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES))
            : $this->draw($results);

        return self::SUCCESS;
    }

    private function report(string $directory, array $results): array
    {
        $coming = array_values(array_filter($results, fn (array $entry) => $entry['skip'] === null));
        $skipped = array_values(array_filter($results, fn (array $entry) => $entry['skip'] !== null));

        return [
            'source' => $directory,
            'dry_run' => (bool) $this->option('dry-run'),
            $this->option('dry-run') ? 'would_import' : 'imported' => array_map(fn (array $entry) => [
                'name' => $entry['name'],
                'driver' => $entry['driver'],
                'group' => $entry['group'],
                'tag' => $entry['tag'],
                'over_ssh' => $entry['over_ssh'],
                'password' => $entry['password'] ?? 'from Keychain, after macOS asks',
            ], $coming),
            'skipped' => array_map(fn (array $entry) => [
                'name' => $entry['name'],
                'reason' => $entry['skip'],
            ], $skipped),
        ];
    }

    private function draw(array $results): void
    {
        $dryRun = (bool) $this->option('dry-run');

        foreach ($results as $entry) {
            if ($entry['skip'] !== null) {
                $this->line("  <fg=gray>skip</>   {$entry['name']}  <fg=gray>{$entry['skip']}</>");

                continue;
            }

            $where = implode(' · ', array_filter([$entry['driver'], $entry['group'], $entry['tag'], $entry['over_ssh'] ? 'ssh' : null]));
            $password = $entry['password'] ?? 'from Keychain, after macOS asks';

            $this->line(sprintf(
                '  <fg=green>%s</>  %s  <fg=gray>%s · password %s</>',
                $dryRun ? 'would' : 'added',
                $entry['name'],
                $where,
                $password,
            ));
        }

        $count = count(array_filter($results, fn (array $entry) => $entry['skip'] === null));

        $this->newLine();
        $this->line($dryRun
            ? "  {$count} to bring over. Run <fg=green>tql tableplus</> to do it."
            : "  Added {$count}. Run <fg=green>tql</> to see them.");
    }

    private function refuse(string $message): int
    {
        if ($this->wantsJson()) {
            $this->line(json_encode(['error' => $message], JSON_UNESCAPED_SLASHES));
        } else {
            $this->error($message);
        }

        return self::FAILURE;
    }

    private function wantsJson(): bool
    {
        if ($this->option('json')) {
            return true;
        }

        return ! $this->option('table') && ! (function_exists('stream_isatty') && @stream_isatty(STDOUT));
    }
}
