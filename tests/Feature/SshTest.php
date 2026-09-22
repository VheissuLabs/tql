<?php

use App\Models\Connection;
use App\Ssh\Tunnel;
use App\Support\KeyFiles;
use App\Tui\ConnectionForm;
use App\Tui\ConnectionPicker;
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

it('hides the ssh block behind a switch', function () {
    $connection = tunnelled(['ssh_host' => null, 'ssh_user' => null]);

    $form = new ConnectionForm($connection);

    expect($form->overSsh())->toBeFalse()
        ->and(array_keys($form->fields()))->toContain('over_ssh')
        ->and(array_keys($form->fields()))->not->toContain('ssh_host');

    $form->values['over_ssh'] = 'yes';

    expect(array_keys($form->fields()))
        ->toContain('ssh_host')
        ->toContain('ssh_user')
        ->toContain('ssh_port')
        ->toContain('ssh_key')
        ->toContain('ssh_password');
});

it('opens the switch already on for a connection that tunnels', function () {
    expect((new ConnectionForm(tunnelled()))->overSsh())->toBeTrue();
});

it('clears the ssh settings when the switch is turned off', function () {
    $connection = tunnelled();

    $form = new ConnectionForm($connection);

    expect($form->overSsh())->toBeTrue();

    $form->values['over_ssh'] = 'no';

    expect($form->save())->toBeNull();

    $saved = $connection->fresh();

    expect($saved->ssh_host)->toBeNull()
        ->and($saved->ssh_user)->toBeNull()
        ->and($saved->usesSsh())->toBeFalse();
});

it('says where the key comes from when none is set', function () {
    $form = new ConnectionForm(tunnelled());

    expect($form->display('ssh_key'))->toBe('agent or ~/.ssh/config')
        ->and($form->display('ssh_port'))->toBe('22');
});

it('saves the ssh settings', function () {
    $connection = tunnelled(['ssh_host' => null, 'ssh_user' => null]);

    $form = new ConnectionForm($connection);
    $form->values['over_ssh'] = 'yes';
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

it('finds the private keys in ~/.ssh and skips what is not one', function () {
    $keys = KeyFiles::sshKeys();

    foreach ($keys as $key) {
        expect($key)->not->toEndWith('.pub')
            ->and(basename($key))->not->toBe('known_hosts')
            ->and(basename($key))->not->toBe('config');
    }
});

it('shortens a key path to the home directory', function () {
    expect(KeyFiles::shorten(getenv('HOME').'/.ssh/id_rsa'))->toBe('~/.ssh/id_rsa')
        ->and(KeyFiles::shorten('/etc/ssl/ca.pem'))->toBe('/etc/ssl/ca.pem');
});

it('offers the keys it found plus a way to type one', function () {
    $form = new ConnectionForm(tunnelled());

    $files = $form->files('ssh_key');

    expect(end($files))->toBe(ConnectionForm::TYPE_IT);
});

it('picks a key from the list instead of typing it', function () {
    // Build a home with a key in it, so the test does not depend on whether
    // the machine running it happens to have one.
    $home = sys_get_temp_dir().'/tql-home-'.uniqid();

    mkdir($home.'/.ssh', 0700, true);
    file_put_contents($home.'/.ssh/id_test', "-----BEGIN OPENSSH PRIVATE KEY-----\nnot really\n");
    file_put_contents($home.'/.ssh/id_test.pub', 'ssh-ed25519 AAAA not-a-key');
    file_put_contents($home.'/.ssh/known_hosts', 'example.com ssh-ed25519 AAAA');

    $previous = getenv('HOME');
    putenv("HOME={$home}");

    $form = new ConnectionForm(tunnelled(['ssh_key' => null]));

    $form->values['over_ssh'] = 'yes';
    $form->values['ssh_host'] = 'bastion.example.com';

    while ($form->currentKey() !== 'ssh_key') {
        $form->move(1);
    }

    $form->openFilePicker();

    expect($form->picker)->not->toBeNull()
        ->and($form->picker->title)->toBe('SSH KEY')
        // The key is offered; the public key and known_hosts are not.
        ->and($form->picker->options)->toBe(['~/.ssh/id_test', ConnectionForm::TYPE_IT]);

    $form->chooseFile();

    expect($form->picker)->toBeNull()
        ->and($form->values['ssh_key'])->toBe('~/.ssh/id_test');

    putenv($previous === false ? 'HOME' : "HOME={$previous}");
});

it('falls back to typing a path', function () {
    $form = new ConnectionForm(tunnelled());

    $form->values['over_ssh'] = 'yes';
    $form->values['ssh_host'] = 'bastion.example.com';

    while ($form->currentKey() !== 'ssh_key') {
        $form->move(1);
    }

    $form->openFilePicker();

    $form->picker->index = array_search(ConnectionForm::TYPE_IT, $form->picker->matches(), true);

    $form->chooseFile();

    expect($form->picker)->toBeNull()
        ->and($form->editing)->toBeTrue();
});

it('expands a chosen key when it opens the tunnel', function () {
    $connection = tunnelled(['ssh_key' => '~/.ssh/id_ed25519']);

    $tunnel = (new ReflectionClass(Tunnel::class))->newInstanceWithoutConstructor();

    (new ReflectionProperty(Tunnel::class, 'connection'))->setValue($tunnel, $connection);
    (new ReflectionProperty(Tunnel::class, 'port'))->setValue($tunnel, 1234);

    $command = (new ReflectionMethod(Tunnel::class, 'command'))->invoke($tunnel);

    expect($command)->toContain(getenv('HOME').'/.ssh/id_ed25519')
        ->and($command)->not->toContain('~/.ssh/id_ed25519');
});

it('writes an askpass helper only when there is a password', function () {
    $without = tunnelled();
    $with = tunnelled(['name' => 'pw'.uniqid(), 'ssh_password' => 'hunter2']);

    $environment = function (Connection $connection) {
        $tunnel = (new ReflectionClass(Tunnel::class))->newInstanceWithoutConstructor();

        (new ReflectionProperty(Tunnel::class, 'connection'))->setValue($tunnel, $connection);

        return [(new ReflectionMethod(Tunnel::class, 'environment'))->invoke($tunnel), $tunnel];
    };

    [$none] = $environment($without);

    expect($none)->toBeNull();

    [$env, $tunnel] = $environment($with);

    expect($env)->toHaveKey('SSH_ASKPASS')
        ->and($env['SSH_ASKPASS_REQUIRE'])->toBe('force')
        ->and(is_file($env['SSH_ASKPASS']))->toBeTrue();

    // The password lives in a file only the user can read.
    $secret = dirname($env['SSH_ASKPASS']).'/secret';

    expect(file_get_contents($secret))->toBe('hunter2')
        ->and(substr(sprintf('%o', fileperms($secret)), -3))->toBe('600');

    (new ReflectionMethod(Tunnel::class, 'forgetAskpass'))->invoke($tunnel);

    expect(is_file($secret))->toBeFalse();
});

it('keeps the form open when the key list is closed', function () {
    $picker = new ConnectionPicker(collect());

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    $picker->form->values['over_ssh'] = 'yes';
    $picker->form->values['ssh_host'] = 'bastion.example.com';
    $picker->form->index = array_search('ssh_key', $picker->form->keys(), true);

    $picker->emit('key', "\n");

    expect($picker->form?->picker)->not->toBeNull();

    $picker->emit('key', "\e");

    // Escape closes the list, not the form behind it.
    expect($picker->form)->not->toBeNull()
        ->and($picker->form->picker)->toBeNull();

    $picker->emit('key', "\e");

    expect($picker->form)->toBeNull();
});

it('renders the key list without blowing up', function () {
    $picker = new ConnectionPicker(collect());

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== 'mysql') {
        $picker->form->cycleDriver();
    }

    $picker->form->values['over_ssh'] = 'yes';
    $picker->form->values['ssh_host'] = 'bastion.example.com';
    $picker->form->index = array_search('ssh_key', $picker->form->keys(), true);

    $picker->emit('key', "\n");

    $render = new ReflectionMethod($picker, 'renderTheme');
    $render->setAccessible(true);

    expect(preg_replace('/\e\[[0-9;]*m/', '', $render->invoke($picker)))
        ->toContain('SSH KEY')
        ->toContain('type a path');
});

it('walks every field of every driver without closing the form', function (string $driver) {
    $picker = new ConnectionPicker(collect());

    $picker->emit('key', 'n');

    while ($picker->form->driver() !== $driver) {
        $picker->form->cycleDriver();
    }

    $picker->form->values['ssl_mode'] = 'require';
    $picker->form->values['over_ssh'] = 'yes';
    $picker->form->values['ssh_host'] = 'bastion.example.com';

    $render = new ReflectionMethod($picker, 'renderTheme');
    $render->setAccessible(true);

    foreach (array_keys($picker->form->fields()) as $index => $field) {
        expect($picker->form)->not->toBeNull("the form closed before {$field}");

        $picker->form->index = min($index, count($picker->form->keys()) - 1);

        $render->invoke($picker);

        $picker->emit('key', "\n");
        $render->invoke($picker);

        if ($picker->form?->picker !== null || $picker->form?->editor !== null) {
            $picker->emit('key', "\e");
            $render->invoke($picker);
        }
    }

    expect($picker->form)->not->toBeNull();
})->with(['sqlite', 'mysql', 'pgsql']);
