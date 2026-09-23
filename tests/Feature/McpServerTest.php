<?php

use App\Mcp\Servers\TqlServer;
use Laravel\Mcp\Server\Contracts\Transport;

it('reports the version tql was built as, without the tag v', function () {
    config(['app.version' => 'v9.8.7']);

    $transport = new class implements Transport
    {
        public array $sent = [];

        public function onReceive(Closure $handler): void {}

        public function run() {}

        public function send(string $message): void
        {
            $this->sent[] = $message;
        }

        public function stream(Closure $stream): void {}
    };

    $server = new TqlServer($transport);
    $server->start();
    $server->handle(json_encode([
        'jsonrpc' => '2.0',
        'id' => 1,
        'method' => 'initialize',
        'params' => [
            'protocolVersion' => '2025-06-18',
            'capabilities' => [],
            'clientInfo' => ['name' => 'test', 'version' => '1'],
        ],
    ]));

    $reply = json_decode($transport->sent[0] ?? '{}', true);

    expect($reply['result']['serverInfo']['name'] ?? null)->toBe('tql')
        ->and($reply['result']['serverInfo']['version'] ?? null)->toBe('9.8.7');
});
