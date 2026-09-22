<?php

namespace App\Ssh;

use App\Models\Connection;
use App\Support\KeyFiles;

/**
 * Everything the rest of the application needs to know about SSH.
 *
 * Which columns hold it, which of them are files, what the defaults are, and
 * whether a given connection uses it. The model, the form and the connection
 * manager all ask here rather than each knowing the field names.
 */
class Settings
{
    /** The columns, in the order the form shows them. */
    public const FIELDS = [
        'ssh_host' => 'Server',
        'ssh_port' => 'Port',
        'ssh_user' => 'User',
        'ssh_key' => 'Key',
        'ssh_password' => 'Password',
    ];

    /**
     * The switch that reveals the rest. It is not a column: it is derived
     * from whether there is a host, and clears the rest when turned off.
     */
    public const TOGGLE = 'over_ssh';

    public const HOST = 'ssh_host';

    /** Held encrypted, like the database password. */
    public const SECRET = 'ssh_password';

    /** A path to a file on this machine. */
    public const FILE = 'ssh_key';

    public const DEFAULT_PORT = 22;

    public static function used(Connection $connection): bool
    {
        return $connection->driver !== 'sqlite'
            && trim((string) $connection->ssh_host) !== '';
    }

    /**
     * @return array<string, string>
     */
    public static function details(): array
    {
        return self::FIELDS;
    }

    /**
     * The columns to blank when the switch is turned off, so a connection
     * does not keep tunnelling through something you told it to stop using.
     *
     * @return array<string, null>
     */
    public static function cleared(): array
    {
        return array_fill_keys(array_keys(self::FIELDS), null);
    }

    /**
     * What to show for a field that has not been filled in, so the form says
     * what will happen rather than leaving a blank.
     */
    public static function placeholder(string $field): ?string
    {
        return match ($field) {
            'ssh_port' => (string) self::DEFAULT_PORT,
            'ssh_key' => 'agent or ~/.ssh/config',
            default => null,
        };
    }

    /**
     * @return array<int, string>
     */
    public static function keys(): array
    {
        return KeyFiles::sshKeys();
    }

    public static function describe(Connection $connection): string
    {
        return 'ssh '.($connection->ssh_user ? $connection->ssh_user.'@' : '').$connection->ssh_host;
    }
}
