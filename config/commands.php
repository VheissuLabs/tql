<?php

use App\Commands\BrowseCommand;
use Illuminate\Console\Scheduling\ScheduleFinishCommand;
use Illuminate\Console\Scheduling\ScheduleListCommand;
use Illuminate\Console\Scheduling\ScheduleRunCommand;
use Illuminate\Database\Console\Factories\FactoryMakeCommand;
use Illuminate\Database\Console\Migrations\FreshCommand;
use Illuminate\Database\Console\Migrations\InstallCommand as MigrateInstallCommand;
use Illuminate\Database\Console\Migrations\MigrateCommand;
use Illuminate\Database\Console\Migrations\MigrateMakeCommand;
use Illuminate\Database\Console\Migrations\RefreshCommand;
use Illuminate\Database\Console\Migrations\ResetCommand;
use Illuminate\Database\Console\Migrations\RollbackCommand;
use Illuminate\Database\Console\Migrations\StatusCommand;
use Illuminate\Database\Console\Seeds\SeedCommand;
use Illuminate\Database\Console\Seeds\SeederMakeCommand;
use Illuminate\Database\Console\WipeCommand;
use Illuminate\Foundation\Console\ModelMakeCommand;
use Illuminate\Foundation\Console\VendorPublishCommand;
use Laravel\Ai\Console\Commands\MakeAgentCommand;
use Laravel\Ai\Console\Commands\MakeAgentMiddlewareCommand;
use Laravel\Ai\Console\Commands\MakeToolCommand as MakeAiToolCommand;
use Laravel\Mcp\Console\Commands\InspectorCommand;
use Laravel\Mcp\Console\Commands\MakeAppResourceCommand;
use Laravel\Mcp\Console\Commands\MakePromptCommand;
use Laravel\Mcp\Console\Commands\MakeResourceCommand;
use Laravel\Mcp\Console\Commands\MakeServerCommand;
use Laravel\Mcp\Console\Commands\MakeToolCommand;
use LaravelZero\Framework\Commands\StubPublishCommand;
use NunoMaduro\LaravelConsoleSummary\SummaryCommand;
use Symfony\Component\Console\Command\DumpCompletionCommand;
use Symfony\Component\Console\Command\HelpCommand;

return [

    /*
    |--------------------------------------------------------------------------
    | Default Command
    |--------------------------------------------------------------------------
    |
    | Laravel Zero will always run the command specified below when no command name is
    | provided. Consider update the default command for single command applications.
    | You cannot pass arguments to the default command because they are ignored.
    |
    */

    'default' => BrowseCommand::class,

    /*
    |--------------------------------------------------------------------------
    | Commands Paths
    |--------------------------------------------------------------------------
    |
    | This value determines the "paths" that should be loaded by the console's
    | kernel. Foreach "path" present on the array provided below the kernel
    | will extract all "Illuminate\Console\Command" based class commands.
    |
    */

    'paths' => [app_path('Commands')],

    /*
    |--------------------------------------------------------------------------
    | Added Commands
    |--------------------------------------------------------------------------
    |
    | You may want to include a single command class without having to load an
    | entire folder. Here you can specify which commands should be added to
    | your list of commands. The console's kernel will try to load them.
    |
    */

    'add' => [
        //
    ],

    /*
    |--------------------------------------------------------------------------
    | Hidden Commands
    |--------------------------------------------------------------------------
    |
    | Your application commands will always be visible on the application list
    | of commands. But you can still make them "hidden" specifying an array
    | of commands below. All "hidden" commands can still be run/executed.
    |
    */

    // The binary is one TUI with two side doors. The rest is framework
    // plumbing — migrations tql runs for itself, seeders, scaffolding — still
    // runnable, just not worth showing to someone who typed `tql list` to find
    // out what tql does. (Laravel Zero's 'remove' cannot take them: the kernel
    // applies it before Laravel's own providers register their commands.)
    'hidden' => [
        BrowseCommand::class,
        MigrateCommand::class,
        FreshCommand::class,
        RefreshCommand::class,
        ResetCommand::class,
        RollbackCommand::class,
        MigrateInstallCommand::class,
        StatusCommand::class,
        SeedCommand::class,
        WipeCommand::class,
        FactoryMakeCommand::class,
        MigrateMakeCommand::class,
        ModelMakeCommand::class,
        SeederMakeCommand::class,
        MakeAgentCommand::class,
        MakeAgentMiddlewareCommand::class,
        MakeAiToolCommand::class,
        MakeAppResourceCommand::class,
        MakePromptCommand::class,
        MakeResourceCommand::class,
        MakeServerCommand::class,
        MakeToolCommand::class,
        InspectorCommand::class,
        SummaryCommand::class,
        DumpCompletionCommand::class,
        HelpCommand::class,
        ScheduleRunCommand::class,
        ScheduleListCommand::class,
        ScheduleFinishCommand::class,
        VendorPublishCommand::class,
        StubPublishCommand::class,
    ],

    /*
    |--------------------------------------------------------------------------
    | Removed Commands
    |--------------------------------------------------------------------------
    |
    | Do you have a service provider that loads a list of commands that
    | you don't need? No problem. Laravel Zero allows you to specify
    | below a list of commands that you don't to see in your app.
    |
    */

    'remove' => [
        //
    ],

];
