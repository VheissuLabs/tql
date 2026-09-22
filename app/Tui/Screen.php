<?php

namespace App\Tui;

enum Screen
{
    case Connections;
    case NewConnection;
    case Tables;
    case Rows;
    case Sql;
    case Quit;
}
