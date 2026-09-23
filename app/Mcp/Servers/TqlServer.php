<?php

namespace App\Mcp\Servers;

use App\Mcp\Tools\DescribeTableTool;
use App\Mcp\Tools\ListConnectionsTool;
use App\Mcp\Tools\ListTablesTool;
use App\Mcp\Tools\RunQueryTool;
use Laravel\Mcp\Server;
use Laravel\Mcp\Server\Attributes\Instructions;
use Laravel\Mcp\Server\Attributes\Name;
use Laravel\Mcp\Server\Attributes\Version;

#[Name('tql')]
#[Version('0.4.0')]
#[Instructions(
    'tql exposes the same database connections the user browses in the tql terminal interface. '.
    'Start by listing connections, then list or describe tables before querying. '.
    'Queries are read-only; writes must happen in the tql interface.'
)]
class TqlServer extends Server
{
    protected array $tools = [
        ListConnectionsTool::class,
        ListTablesTool::class,
        DescribeTableTool::class,
        RunQueryTool::class,
    ];
}
