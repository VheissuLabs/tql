# Renders docs/demo.gif. From the repository root: vhs docs/demo.tape

Output docs/demo.gif

Set Shell "bash"
Set FontSize 15
Set Width 1400
Set Height 820
Set Padding 16
Set Theme "TokyoNight"
Set TypingSpeed 45ms

Hide
Type `export XDG_CONFIG_HOME=$(mktemp -d) && mkdir -p $XDG_CONFIG_HOME/tql`
Enter
Type `printf '# tql configuration 0.6.0\n[ui]\nsql_editor = "vim"\nsql_always = true\nsql_position = "bottom"\nsql_height = 11\ntop_margin = 0\nmouse = false\n' > $XDG_CONFIG_HOME/tql/config.toml`
Enter
Type `alias tql="php $PWD/tql" && cd test-databases && clear`
Enter
Show

Type "tql chinook.db"
Sleep 500ms
Enter
Sleep 1.2s

Type "10j"
Sleep 400ms
Enter
Sleep 1s

Type "l"
Sleep 400ms
Type "f"
Sleep 500ms
Type "love"
Sleep 400ms
Enter
Sleep 300ms
Enter
Sleep 1.2s

Type "l"
Sleep 400ms
Type "o"
Sleep 1.2s

Type "i"
Sleep 1.2s
Tab
Sleep 400ms
Type "jj"
Sleep 300ms
Enter
Sleep 1s
Type "j"
Sleep 300ms
Type "i"
Sleep 1.2s
Tab
Sleep 400ms
Enter
Sleep 800ms
Type "jj"
Sleep 300ms
Enter
Sleep 1.8s
Escape
Sleep 500ms
Escape
Sleep 700ms

Type "s"
Sleep 500ms
Type "ggdG"
Sleep 500ms
Type "i"
Type "select ar.Name, count(*) as tracks"
Enter
Type "from tracks t"
Enter
Type "  join albums al on al.AlbumId = t.AlbumId"
Enter
Type "join artists ar on ar.ArtistId = al.ArtistId"
Enter
Backspace 2
Type "group by ar.Name"
Enter
Type "order by tracks desc"
Enter
Type "limit 10"
Escape
Sleep 500ms
Type ":r"
Sleep 500ms
Enter
Sleep 1.8s

Type "ciw"
Sleep 400ms
Type "5"
Escape
Sleep 500ms
Type ":r"
Sleep 500ms
Enter
Sleep 2.5s
