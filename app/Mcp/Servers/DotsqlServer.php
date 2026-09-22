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

#[Name('dotsql')]
#[Version('0.1.0')]
#[Instructions(
    'dotsql exposes the same database connections the user browses in the dotsql terminal interface. '.
    'Start by listing connections, then list or describe tables before querying. '.
    'Queries are read-only; writes must happen in the dotsql interface.'
)]
class DotsqlServer extends Server
{
    protected array $tools = [
        ListConnectionsTool::class,
        ListTablesTool::class,
        DescribeTableTool::class,
        RunQueryTool::class,
    ];
}
