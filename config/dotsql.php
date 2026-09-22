<?php

return [

    'theme' => [

        'border' => 'dim',

        'focus_border' => 'cyan',

        'focus_title' => 'cyan',

        'grid' => 'dim',

        'cursor' => 'default',

        'selection' => 'default',

    ],

    'ui' => [

        'top_margin' => 1,

        'row_style' => 'marker',

        'sql_position' => 'top',

        'sql_always' => false,

        'sql_height' => 0,

        'export_path' => null,

        'sidebar_width' => 24,

        'mouse_row_offset' => env('DOTSQL_MOUSE_ROW_OFFSET', 0),

        'mouse_column_offset' => env('DOTSQL_MOUSE_COLUMN_OFFSET', 0),

    ],

];
