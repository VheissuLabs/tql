<?php

use App\Models\Setting;
use App\Updates\Installation;
use App\Updates\UpdateCheck;
use App\Updates\UpdateFailed;
use App\Updates\Updater;
use App\Updates\Version;
use Illuminate\Support\Facades\Http;

beforeEach(function () {
    Setting::query()->whereIn('key', [UpdateCheck::CHECKED, UpdateCheck::LATEST, UpdateCheck::INSTALLED])->delete();
    config(['tql.updates.check' => true, 'tql.updates.automatic' => true]);
});

function fakeRelease(string $version, string $binary, ?string $sums = null): void
{
    $asset = Updater::asset();

    Http::fake([
        'api.github.com/repos/VheissuLabs/tql/releases/latest' => Http::response(['tag_name' => "v{$version}"]),
        "github.com/VheissuLabs/tql/releases/download/v{$version}/SHA256SUMS" => $sums === null
            ? Http::response('', 404)
            : Http::response($sums),
        "github.com/VheissuLabs/tql/releases/download/v{$version}/{$asset}" => Http::response($binary),
    ]);
}

function fakeBinary(string $version): string
{
    return "#!/bin/sh\necho 'Tql v{$version}'\n";
}

function installedAt(): string
{
    $dir = sys_get_temp_dir().'/tql-update-'.uniqid();
    mkdir($dir);
    file_put_contents("{$dir}/tql", fakeBinary('0.7.0'));
    chmod("{$dir}/tql", 0755);

    return "{$dir}/tql";
}

it('reads a version out of what tql and GitHub call it', function (string $raw, ?string $version) {
    expect(Version::of($raw))->toBe($version);
})->with([
    ['0.7.0', '0.7.0'],
    ['v0.7.0', '0.7.0'],
    ['Tql v0.7.0', '0.7.0'],
    ['v0.7.0-3-gabc1234', '0.7.0'],
    ['dev', null],
]);

it('knows how tql was installed from where the binary is', function (string $sapi, string $path, string $how) {
    expect(Installation::of($sapi, $path)->how)->toBe($how);
})->with([
    ['micro', '/opt/homebrew/Cellar/tql/0.7.0/bin/tql', Installation::HOMEBREW],
    ['micro', '/home/linuxbrew/.linuxbrew/Cellar/tql/0.7.0/bin/tql', Installation::HOMEBREW],
    ['micro', '/usr/bin/tql', Installation::PACKAGE],
    ['micro', '/usr/local/bin/tql', Installation::BY_ITSELF],
    ['micro', '/Users/someone/.local/bin/tql', Installation::BY_ITSELF],
    ['cli', '/Users/someone/Code/tql/tql', Installation::PHP],
]);

it('tells each kind of install how to upgrade', function (string $how, string $says) {
    expect((new Installation($how, '/x/tql'))->upgrade())->toContain($says);
})->with([
    [Installation::HOMEBREW, 'brew upgrade tql'],
    [Installation::PACKAGE, '.deb'],
    [Installation::BY_ITSELF, 'tql update'],
    [Installation::PHP, 'releases'],
]);

it('replaces the binary with the release once its checksum matches', function () {
    $target = installedAt();
    $binary = fakeBinary('0.8.0');

    fakeRelease('0.8.0', $binary, hash('sha256', $binary).'  '.Updater::asset()."\n");

    (new Updater)->install('0.8.0', $target);

    expect(file_get_contents($target))->toBe($binary)
        ->and(is_executable($target))->toBeTrue()
        ->and(glob(dirname($target).'/.tql-*'))->toBe([]);
});

it('refuses a binary that does not match its checksum and leaves the old one alone', function () {
    $target = installedAt();

    fakeRelease('0.8.0', fakeBinary('0.8.0'), str_repeat('0', 64).'  '.Updater::asset()."\n");

    expect(fn () => (new Updater)->install('0.8.0', $target))->toThrow(UpdateFailed::class, 'checksum');
    expect(file_get_contents($target))->toBe(fakeBinary('0.7.0'));
});

it('refuses a release that publishes no checksums', function () {
    $target = installedAt();

    fakeRelease('0.8.0', fakeBinary('0.8.0'));

    expect(fn () => (new Updater)->install('0.8.0', $target))->toThrow(UpdateFailed::class, 'no checksums');
    expect(file_get_contents($target))->toBe(fakeBinary('0.7.0'));
});

it('refuses a binary that does not say it is the version it was downloaded as', function () {
    $target = installedAt();
    $binary = fakeBinary('0.6.0');

    fakeRelease('0.8.0', $binary, hash('sha256', $binary).'  '.Updater::asset()."\n");

    expect(fn () => (new Updater)->install('0.8.0', $target))->toThrow(UpdateFailed::class);
    expect(file_get_contents($target))->toBe(fakeBinary('0.7.0'));
});

it('checks once a day at most', function () {
    expect(UpdateCheck::due())->toBeTrue();

    Setting::write(UpdateCheck::CHECKED, (string) now()->subHours(2)->timestamp);
    expect(UpdateCheck::due())->toBeFalse();

    Setting::write(UpdateCheck::CHECKED, (string) now()->subDays(2)->timestamp);
    expect(UpdateCheck::due())->toBeTrue();

    config(['tql.updates.check' => false]);
    expect(UpdateCheck::due())->toBeFalse();
});

it('updates by itself when it can, and says a restart will use it', function () {
    $target = installedAt();
    $binary = fakeBinary('0.8.0');

    fakeRelease('0.8.0', $binary, hash('sha256', $binary).'  '.Updater::asset()."\n");

    $installation = new Installation(Installation::BY_ITSELF, $target);

    UpdateCheck::run('0.7.0', $installation, new Updater);

    expect(file_get_contents($target))->toBe($binary)
        ->and(UpdateCheck::notice('0.7.0', $installation))->toBe('tql 0.8.0 is ready · restart to use it')
        ->and(UpdateCheck::notice('0.8.0', $installation))->toBeNull();
});

it('only says what to run when tql cannot update itself', function () {
    fakeRelease('0.8.0', fakeBinary('0.8.0'), 'irrelevant');

    $installation = new Installation(Installation::HOMEBREW, '/opt/homebrew/Cellar/tql/0.7.0/bin/tql');

    UpdateCheck::run('0.7.0', $installation, new Updater);

    expect(UpdateCheck::notice('0.7.0', $installation))->toBe('tql 0.8.0 is out · brew upgrade tql');
    Http::assertNotSent(fn ($request) => str_contains($request->url(), '/releases/download/'));
});

it('only says a new version is out when automatic updates are off', function () {
    config(['tql.updates.automatic' => false]);

    $target = installedAt();
    fakeRelease('0.8.0', fakeBinary('0.8.0'), 'irrelevant');

    $installation = new Installation(Installation::BY_ITSELF, $target);

    UpdateCheck::run('0.7.0', $installation, new Updater);

    expect(file_get_contents($target))->toBe(fakeBinary('0.7.0'))
        ->and(UpdateCheck::notice('0.7.0', $installation))->toBe('tql 0.8.0 is out · tql update');
});

it('says nothing when it is already the latest, or GitHub cannot be reached', function () {
    $installation = new Installation(Installation::BY_ITSELF, installedAt());

    fakeRelease('0.7.0', fakeBinary('0.7.0'), 'irrelevant');
    UpdateCheck::run('0.7.0', $installation, new Updater);
    expect(UpdateCheck::notice('0.7.0', $installation))->toBeNull();

    Http::fake(['*' => Http::response('', 500)]);
    UpdateCheck::run('0.7.0', $installation, new Updater);
    expect(UpdateCheck::notice('0.7.0', $installation))->toBeNull();
});

it('says from the command line what is out, and how to get it when tql cannot update itself', function () {
    config(['app.version' => 'v0.7.0']);
    fakeRelease('0.8.0', fakeBinary('0.8.0'), 'irrelevant');

    $this->artisan('update')
        ->expectsOutputToContain('tql 0.8.0 is out')
        ->assertExitCode(0);
});

it('says so from the command line when it is already the latest', function () {
    config(['app.version' => 'v0.8.0']);
    fakeRelease('0.8.0', fakeBinary('0.8.0'), 'irrelevant');

    $this->artisan('update')
        ->expectsOutputToContain('0.8.0 is the latest')
        ->assertExitCode(0);
});
