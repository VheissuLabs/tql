<?php

use App\Database\Tunnel;
use App\Models\Connection;
use App\Tui\ConnectionForm;
use Illuminate\Support\Facades\Artisan;

beforeEach(function () {
    Artisan::call('migrate', ['--force' => true]);
    Connection::query()->delete();
});

function tunnelled(array $attributes = []): Connection
{
    return Connection::create(array_merge([
        'name' => 'prod'.uniqid(),
        'driver' => 'mysql',
        'host' => 'db.internal',
        'port' => 3306,
        'database' => 'shop',
        'username' => 'alice',
        'ssh_host' => 'bastion.example.com',
        'ssh_user' => 'karl',
    ], $attributes));
}

it('knows when a connection goes through ssh', function () {
    expect(tunnelled()->usesSsh())->toBeTrue()
        ->and(tunnelled(['ssh_host' => null])->usesSsh())->toBeFalse()
        ->and(tunnelled(['ssh_host' => '  '])->usesSsh())->toBeFalse();
});

it('never tunnels a sqlite file', function () {
    $connection = Connection::create([
        'name' => 'local'.uniqid(),
        'driver' => 'sqlite',
        'database' => '/tmp/app.sqlite',
        'ssh_host' => 'bastion.example.com',
    ]);

    expect($connection->usesSsh())->toBeFalse();
});

it('says in the connection list that it goes through ssh', function () {
    expect(tunnelled()->describe())->toContain('ssh karl@bastion.example.com')
        ->and(tunnelled(['ssh_user' => null])->describe())->toContain('ssh bastion.example.com');
});

it('builds an ssh command that forwards the database port', function () {
    $connection = tunnelled();

    $tunnel = (new ReflectionClass(Tunnel::class))->newInstanceWithoutConstructor();

    (new ReflectionProperty(Tunnel::class, 'connection'))->setValue($tunnel, $connection);
    (new ReflectionProperty(Tunnel::class, 'port'))->setValue($tunnel, 54321);

    $command = (new ReflectionMethod(Tunnel::class, 'command'))->invoke($tunnel);

    expect($command[0])->toBe('ssh')
        ->and($command)->toContain('-N')
        ->and($command)->toContain('54321:db.internal:3306')
        ->and(end($command))->toBe('karl@bastion.example.com');
});

it('passes a key file and a port when they are set', function () {
    $connection = tunnelled(['ssh_key' => '~/.ssh/id_prod', 'ssh_port' => 2222]);

    $tunnel = (new ReflectionClass(Tunnel::class))->newInstanceWithoutConstructor();

    (new ReflectionProperty(Tunnel::class, 'connection'))->setValue($tunnel, $connection);
    (new ReflectionProperty(Tunnel::class, 'port'))->setValue($tunnel, 54321);

    $command = (new ReflectionMethod(Tunnel::class, 'command'))->invoke($tunnel);

    expect($command)->toContain('-i')
        ->and($command)->toContain(getenv('HOME').'/.ssh/id_prod')
        ->and($command)->toContain('-p')
        ->and($command)->toContain('2222');
});

it('expands a key path that starts with a tilde', function () {
    expect(Tunnel::expand('~/.ssh/id_rsa'))->toBe(getenv('HOME').'/.ssh/id_rsa')
        ->and(Tunnel::expand('/etc/ssh/key'))->toBe('/etc/ssh/key');
});

it('asks the system for a port that is free', function () {
    $port = Tunnel::freePort();

    expect($port)->toBeGreaterThan(1024)
        ->and($port)->toBeLessThan(65536);

    // Free means we can actually bind it.
    $socket = @stream_socket_server("tcp://127.0.0.1:{$port}", $code, $message);

    expect($socket)->not->toBeFalse();

    fclose($socket);
});

it('reuses one tunnel per destination', function () {
    $one = tunnelled();
    $two = tunnelled(['name' => 'second'.uniqid()]);

    expect(Tunnel::keyFor($one))->toBe(Tunnel::keyFor($two));

    $other = tunnelled(['name' => 'other'.uniqid(), 'host' => 'other.internal']);

    expect(Tunnel::keyFor($other))->not->toBe(Tunnel::keyFor($one));
});

it('offers the ssh host on a server connection and hides the rest until it is set', function () {
    $connection = tunnelled(['ssh_host' => null, 'ssh_user' => null]);

    $form = new ConnectionForm($connection);

    expect(array_keys($form->fields()))->toContain('ssh_host')
        ->and(array_keys($form->fields()))->not->toContain('ssh_user');

    $form->values['ssh_host'] = 'bastion.example.com';

    expect(array_keys($form->fields()))
        ->toContain('ssh_user')
        ->toContain('ssh_port')
        ->toContain('ssh_key');
});

it('says where the key comes from when none is set', function () {
    $form = new ConnectionForm(tunnelled());

    expect($form->display('ssh_key'))->toBe('agent or ~/.ssh/config')
        ->and($form->display('ssh_port'))->toBe('22');
});

it('saves the ssh settings', function () {
    $connection = tunnelled(['ssh_host' => null, 'ssh_user' => null]);

    $form = new ConnectionForm($connection);
    $form->values['ssh_host'] = 'bastion.example.com';
    $form->values['ssh_user'] = 'karl';
    $form->values['ssh_port'] = '2222';

    expect($form->save())->toBeNull();

    $saved = $connection->fresh();

    expect($saved->ssh_host)->toBe('bastion.example.com')
        ->and($saved->ssh_user)->toBe('karl')
        ->and($saved->ssh_port)->toBe(2222)
        ->and($saved->usesSsh())->toBeTrue();
});
