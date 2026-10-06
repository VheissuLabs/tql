<?php

$tql = getenv('TQL_PATH') ?: getenv('HOME').'/Code/tql';
$cases = json_decode(file_get_contents($argv[1]), true);
$out = $argv[2];

require $tql.'/vendor/autoload.php';

@mkdir($out, 0777, true);

foreach ($cases as $case) {
    $name = $case['name'];

    if ($case['kind'] === 'version') {
        file_put_contents("{$out}/{$name}.toml", App\Support\ConfigTemplate::VERSION);

        continue;
    }

    if ($case['kind'] === 'template') {
        file_put_contents("{$out}/{$name}.toml", App\Support\ConfigTemplate::render());

        continue;
    }

    if ($case['kind'] === 'tidy') {
        $moved = [];
        file_put_contents("{$out}/{$name}.toml", App\Support\ConfigTidy::apply($case['contents'], $moved));
        file_put_contents("{$out}/{$name}.json", json_encode(['moved' => $moved]));

        continue;
    }

    $home = "{$out}/{$name}-home";
    @mkdir("{$home}/tql", 0777, true);
    putenv("XDG_CONFIG_HOME={$home}");

    if ($case['contents'] !== null) {
        file_put_contents("{$home}/tql/config.toml", $case['contents']);
    }

    $result = App\Support\ConfigFile::ensure();
    file_put_contents("{$out}/{$name}.toml", file_get_contents("{$home}/tql/config.toml"));
    file_put_contents("{$out}/{$name}.json", json_encode(['created' => $result['created'], 'added' => $result['added']]));
}
