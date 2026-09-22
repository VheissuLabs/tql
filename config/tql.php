<?php

return [

    /*
    |--------------------------------------------------------------------------
    | Asking for SQL
    |--------------------------------------------------------------------------
    |
    | Which provider and model answer when you press "a". Only the table and
    | column names are sent, never row data. The key comes from the provider's
    | own environment variable, for example ANTHROPIC_API_KEY.
    |
    */

    'ai' => [

        'provider' => env('TQL_AI_PROVIDER', 'auto'),

        'model' => env('TQL_AI_MODEL', ''),

        'timeout' => env('TQL_AI_TIMEOUT', 60),

        // An OpenAI-compatible endpoint: LM Studio, Ollama's OpenAI API,
        // vLLM, a local gateway. Setting this wins over any provider key.
        'url' => env('TQL_AI_URL', ''),

        'key' => env('TQL_AI_KEY', ''),

    ],

    'theme' => [

        'border' => 'dim',

        'focus_border' => 'cyan',

        'focus_title' => 'cyan',

        'grid' => 'dim',

        'modal_border' => 'gray',

        'modal_focus_border' => 'cyan',

        'modal_title' => 'white',

        'modal_focus_title' => 'cyan',

        'cursor' => 'default',

        'selection' => 'default',

        'deleted' => 'red',

        'edited' => 'yellow',

    ],

    /*
    |--------------------------------------------------------------------------
    | Driver icons
    |--------------------------------------------------------------------------
    |
    | Shown beside a connection name. The defaults are Nerd Font devicons; set
    | them to plain characters if your terminal font has no glyph for them.
    |
    */

    'icons' => [

        'mysql' => "\u{e704}",

        'pgsql' => "\u{e76e}",

        'sqlite' => "\u{e7c4}",

        'sqlsrv' => "\u{f1c0}",

        'default' => "\u{f1c0}",

    ],

    'ui' => [

        'top_margin' => 1,

        'row_style' => 'marker',

        'sql_position' => 'top',

        'sql_always' => false,

        'sql_height' => 0,

        'export_path' => null,

        'sidebar_width' => 24,

        'mouse' => env('TQL_MOUSE', true),

        'double_click_ms' => env('TQL_DOUBLE_CLICK_MS', 400),

        // Related rows to load into the row inspector. 0 turns it off.
        'inspect_related' => env('TQL_INSPECT_RELATED', 10),

        'mouse_row_offset' => env('TQL_MOUSE_ROW_OFFSET', 0),

        'mouse_column_offset' => env('TQL_MOUSE_COLUMN_OFFSET', 0),

    ],

];
