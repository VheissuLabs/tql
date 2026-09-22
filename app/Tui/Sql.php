<?php

namespace App\Tui;

class Sql
{
    private const KEYWORDS = [
        'select', 'from', 'where', 'and', 'or', 'not', 'null', 'is', 'in', 'like', 'ilike',
        'between', 'order', 'by', 'group', 'having', 'limit', 'offset', 'asc', 'desc',
        'insert', 'into', 'values', 'update', 'set', 'delete', 'truncate',
        'join', 'left', 'right', 'inner', 'outer', 'full', 'cross', 'on', 'using',
        'as', 'distinct', 'union', 'all', 'with', 'case', 'when', 'then', 'else', 'end',
        'exists', 'create', 'table', 'alter', 'drop', 'index', 'view', 'primary', 'key',
        'foreign', 'references', 'default', 'unique', 'constraint', 'cascade',
        'show', 'explain', 'describe', 'desc', 'pragma', 'begin', 'commit', 'rollback',
        'true', 'false', 'count', 'sum', 'avg', 'min', 'max', 'coalesce', 'cast',
    ];

    public static function tokenise(string $line): array
    {
        $keywords = implode('|', self::KEYWORDS);

        $pattern = '/(--[^\n]*)'.
            '|(\/\*.*?\*\/)'.
            '|(\'(?:\'\'|[^\'])*\')'.
            '|("(?:[^"]|"")*"|`[^`]*`|\[[^\]]*\])'.
            '|\b('.$keywords.')\b'.
            '|(-?\d+(?:\.\d+)?)'.
            '|([=<>!+\-*\/%,;()])/i';

        $tokens = [];
        $offset = 0;

        if (preg_match_all($pattern, $line, $matches, PREG_SET_ORDER | PREG_OFFSET_CAPTURE)) {
            foreach ($matches as $match) {
                $start = $match[0][1];

                if ($start > $offset) {
                    $tokens[] = ['plain', substr($line, $offset, $start - $offset)];
                }

                $tokens[] = static::classify($match);

                $offset = $start + strlen($match[0][0]);
            }
        }

        if ($offset < strlen($line)) {
            $tokens[] = ['plain', substr($line, $offset)];
        }

        return $tokens;
    }

    private static function classify(array $match): array
    {
        $value = fn (int $group) => ($match[$group][0] ?? '') !== '' ? $match[$group][0] : null;

        return match (true) {
            ($comment = $value(1) ?? $value(2)) !== null => ['comment', $comment],
            ($string = $value(3)) !== null => ['string', $string],
            ($identifier = $value(4)) !== null => ['identifier', $identifier],
            ($keyword = $value(5)) !== null => ['keyword', $keyword],
            ($number = $value(6)) !== null => ['number', $number],
            ($operator = $value(7)) !== null => ['operator', $operator],
            default => ['plain', $match[0][0]],
        };
    }
}
