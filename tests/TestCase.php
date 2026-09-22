<?php

namespace Tests;

use LaravelZero\Framework\Testing\TestCase as BaseTestCase;

abstract class TestCase extends BaseTestCase
{
    protected function setUp(): void
    {
        parent::setUp();

        // config/app.php is fixed at 'development' for the binary, so nothing
        // tells Laravel it is running tests — and without that, prompts get
        // neither their interactive terminal nor their test fallbacks.
        config(['app.env' => 'testing']);
        $this->app->instance('env', 'testing');
    }
}
