<?php

use App\Tui\Json;
use App\Tui\RowFormatter;

it('recognises json objects and arrays', function (string $value) {
    expect(Json::looksLikeJson($value))->toBeTrue();
})->with([
    '{"a":1}',
    '[1,2,3]',
    '  {"nested":{"deep":true}}  ',
    '[]',
    '{}',
]);

it('does not mistake other values for json', function (?string $value) {
    expect(Json::looksLikeJson($value))->toBeFalse();
})->with([
    null,
    '',
    'hello',
    '42',
    'true',
    '{not json}',
    '{"unterminated": ',
    'SELECT * FROM users',
]);

it('pretty prints json across lines', function () {
    $pretty = Json::pretty('{"a":1,"b":[2,3]}');

    expect($pretty)->toContain("\n")
        ->and(substr_count($pretty, "\n"))->toBeGreaterThan(3)
        ->and(json_decode($pretty, true))->toBe(['a' => 1, 'b' => [2, 3]]);
});

it('leaves unparseable values alone', function () {
    expect(Json::pretty('not json'))->toBe('not json');
});

it('does not escape slashes or unicode', function () {
    $pretty = Json::pretty('{"url":"https://example.com","name":"café"}');

    expect($pretty)->toContain('https://example.com')
        ->and($pretty)->toContain('café');
});

it('tokenises keys separately from string values', function () {
    $tokens = Json::tokenise('    "email": "karl@example.com",');
    $types = array_column($tokens, 0);

    expect($types)->toContain('key')
        ->and($types)->toContain('string');

    $key = collect($tokens)->first(fn ($t) => $t[0] === 'key');
    $string = collect($tokens)->first(fn ($t) => $t[0] === 'string');

    expect($key[1])->toBe('"email"')
        ->and($string[1])->toBe('"karl@example.com"');
});

it('tokenises numbers and literals', function () {
    $types = fn (string $line) => array_column(Json::tokenise($line), 0);

    expect($types('"n": 9.75,'))->toContain('number')
        ->and($types('"n": -3,'))->toContain('number')
        ->and($types('"n": 1e4,'))->toContain('number')
        ->and($types('"ok": true'))->toContain('literal')
        ->and($types('"ok": null'))->toContain('literal');
});

it('does not treat a string containing a colon as a key', function () {
    $tokens = Json::tokenise('    "https://example.com"');
    $types = array_column($tokens, 0);

    expect($types)->toContain('string')
        ->and($types)->not->toContain('key');
});

it('unescapes slashes and unicode in grid previews', function () {
    $rows = (new RowFormatter)->rows([
        ['payload' => '{"url":"https:\/\/example.com\/hooks","name":"caf\u00e9"}'],
    ]);

    expect($rows[0]['payload'])->toContain('https://example.com/hooks')
        ->and($rows[0]['payload'])->toContain('café')
        ->and($rows[0]['payload'])->not->toContain('\\/')
        ->and($rows[0]['payload'])->not->toContain('\\u00e9');
});

it('leaves non-json strings untouched', function (string $value) {
    $rows = (new RowFormatter)->rows([['v' => $value]]);

    expect($rows[0]['v'])->toBe($value);
})->with([
    'a plain string',
    'C:\\Users\\karl',
    'https://example.com/a\/b',
    '42',
]);

it('keeps json previews on one line', function () {
    $rows = (new RowFormatter)->rows([
        ['payload' => "{\n  \"a\": 1,\n  \"b\": 2\n}"],
    ]);

    expect($rows[0]['payload'])->not->toContain("\n");
});

it('never drops characters while tokenising', function (string $line) {
    $rebuilt = implode('', array_column(Json::tokenise($line), 1));

    expect($rebuilt)->toBe($line);
})->with([
    '    "email": "karl@example.com",',
    '{',
    '}',
    '    "a": 1,',
    '    "a": 1▏,',
    '▏{',
    'not json at all',
    '  trailing spaces   ',
    '    "url": "https://example.com"',
    '    "emoji": "🎉 café",',
    '',
    '   ',
    'truely',
    'nullable',
]);

it('does not highlight words that merely start with a literal', function () {
    $types = array_column(Json::tokenise('"truely": "nullable"'), 0);

    expect($types)->not->toContain('literal');
});
