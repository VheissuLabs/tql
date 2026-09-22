<?php

namespace App\Ssh;

use App\Models\Connection;
use Symfony\Component\Process\Process;

/**
 * An SSH tunnel to a database that is not reachable directly.
 *
 * It shells out to the system ssh rather than speaking the protocol: ssh is
 * already installed, already knows the user's keys, agent and config, and is
 * the thing they would have typed themselves.
 */
class Tunnel
{
    /** @var array<string, self> */
    private static array $open = [];

    private ?Process $process = null;

    private ?string $askpass = null;

    public int $port = 0;

    private function __construct(private Connection $connection) {}

    /**
     * Open a tunnel for a connection, or return the one already open for it.
     */
    public static function for(Connection $connection): self
    {
        $key = static::keyFor($connection);

        if (isset(static::$open[$key]) && static::$open[$key]->running()) {
            return static::$open[$key];
        }

        $tunnel = new self($connection);
        $tunnel->open();

        return static::$open[$key] = $tunnel;
    }

    public static function keyFor(Connection $connection): string
    {
        return implode('|', [
            $connection->ssh_user,
            $connection->ssh_host,
            $connection->ssh_port,
            $connection->host,
            $connection->port,
        ]);
    }

    public static function closeAll(): void
    {
        foreach (static::$open as $tunnel) {
            $tunnel->close();
        }

        static::$open = [];
    }

    public function running(): bool
    {
        return $this->process !== null && $this->process->isRunning();
    }

    public function close(): void
    {
        $this->process?->stop(1);
        $this->process = null;

        $this->forgetAskpass();
    }

    private function open(): void
    {
        $this->port = static::freePort();

        $this->process = new Process($this->command(), null, $this->environment());
        $this->process->setTimeout(null);
        $this->process->start();

        try {
            $this->waitUntilListening();
        } finally {
            // The helper only has to survive the handshake.
            $this->forgetAskpass();
        }
    }

    /**
     * @return array<int, string>
     */
    private function command(): array
    {
        $command = [
            'ssh',
            '-N',
            '-o', 'ExitOnForwardFailure=yes',
            '-o', 'StrictHostKeyChecking=accept-new',
            '-o', 'ServerAliveInterval=30',
            '-L', $this->port.':'.$this->connection->host.':'.($this->connection->port ?: 3306),
        ];

        if ($this->connection->ssh_key) {
            $command[] = '-i';
            $command[] = static::expand((string) $this->connection->ssh_key);
        }

        if ($this->connection->ssh_port && (int) $this->connection->ssh_port !== Settings::DEFAULT_PORT) {
            $command[] = '-p';
            $command[] = (string) $this->connection->ssh_port;
        }

        $command[] = $this->connection->ssh_user
            ? $this->connection->ssh_user.'@'.$this->connection->ssh_host
            : (string) $this->connection->ssh_host;

        return $command;
    }

    /**
     * ssh refuses to read a password from a pipe, so a password is handed
     * over the way ssh asks for one: a helper it runs itself.
     *
     * @return array<string, string>|null
     */
    private function environment(): ?array
    {
        $password = (string) $this->connection->ssh_password;

        if ($password === '') {
            return null;
        }

        $directory = sys_get_temp_dir().'/tql-askpass-'.bin2hex(random_bytes(6));

        mkdir($directory, 0700);

        $secret = $directory.'/secret';
        $script = $directory.'/askpass';

        file_put_contents($secret, $password);
        chmod($secret, 0600);

        file_put_contents($script, "#!/bin/sh\ncat ".escapeshellarg($secret)."\n");
        chmod($script, 0700);

        $this->askpass = $directory;

        return [
            'SSH_ASKPASS' => $script,
            'SSH_ASKPASS_REQUIRE' => 'force',
            'DISPLAY' => (string) (getenv('DISPLAY') ?: ':0'),
        ];
    }

    private function forgetAskpass(): void
    {
        if ($this->askpass === null) {
            return;
        }

        foreach (['secret', 'askpass'] as $file) {
            @unlink($this->askpass.'/'.$file);
        }

        @rmdir($this->askpass);

        $this->askpass = null;
    }

    public static function expand(string $path): string
    {
        return str_starts_with($path, '~/')
            ? (string) getenv('HOME').substr($path, 1)
            : $path;
    }

    /**
     * Wait for the forwarded port to accept a connection, so the first query
     * does not race the tunnel coming up.
     */
    private function waitUntilListening(int $timeoutMs = 10000): void
    {
        $deadline = microtime(true) + ($timeoutMs / 1000);

        while (microtime(true) < $deadline) {
            if (! $this->running()) {
                throw new \RuntimeException($this->reason());
            }

            $socket = @fsockopen('127.0.0.1', $this->port, $code, $message, 0.2);

            if ($socket !== false) {
                fclose($socket);

                return;
            }

            usleep(100000);
        }

        $reason = $this->reason();

        $this->close();

        throw new \RuntimeException($reason === '' ? 'the ssh tunnel did not come up' : $reason);
    }

    private function reason(): string
    {
        $error = trim((string) $this->process?->getErrorOutput());

        return $error === '' ? 'the ssh tunnel did not come up' : 'ssh: '.$error;
    }

    /**
     * A port the operating system says is free, which we then hand to ssh.
     */
    public static function freePort(): int
    {
        $socket = stream_socket_server('tcp://127.0.0.1:0', $code, $message);

        if ($socket === false) {
            return random_int(30000, 60000);
        }

        $name = stream_socket_get_name($socket, false);

        fclose($socket);

        return (int) substr((string) $name, strrpos((string) $name, ':') + 1);
    }
}
