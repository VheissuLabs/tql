<?php

namespace App\TablePlus;

use Symfony\Component\Process\Process;

class Keychain
{
    public const SERVICE = 'com.tableplus.TablePlus';

    public function password(string $account): ?string
    {
        if (PHP_OS_FAMILY !== 'Darwin') {
            return null;
        }

        $process = new Process(['security', 'find-generic-password', '-s', self::SERVICE, '-a', $account, '-w']);
        $process->setTimeout(120);
        $process->run();

        if (! $process->isSuccessful()) {
            return null;
        }

        $password = rtrim($process->getOutput(), "\n");

        return $password === ''
            ? null
            : $password;
    }
}
