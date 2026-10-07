<?php

namespace App\Dump;

class Secrets
{
    private ?string $directory = null;

    private int $written = 0;

    public function write(string $name, string $contents): string
    {
        if ($this->directory === null) {
            $this->directory = sys_get_temp_dir().'/tql-secrets-'.bin2hex(random_bytes(6));

            mkdir($this->directory, 0700);

            register_shutdown_function($this->forget(...));
        }

        $this->written++;

        $path = $this->directory.'/'.$this->written.'-'.$name;

        touch($path);
        chmod($path, 0600);
        file_put_contents($path, $contents);

        return $path;
    }

    public function forget(): void
    {
        if ($this->directory === null) {
            return;
        }

        foreach (glob($this->directory.'/*') ?: [] as $file) {
            @unlink($file);
        }

        @rmdir($this->directory);

        $this->directory = null;
    }
}
