Output target/vhs/tui-tracing-selection.gif

Require cargo
Require vhs

Set Width 1200
Set Height 1024
Set Theme "Aardvark Blue"

Hide
Type "rm -f trace.log"
Enter
Wait
Type "cargo run --example demo"
Enter
Sleep 4s
Show
Sleep 4s

Type "s"
Sleep 2s
Type "j"
Sleep 1s
Type "j"
Sleep 1s
Type "k"
Sleep 1s
Escape
Sleep 1s
Type "j"
Sleep 2s
Type "]"
Sleep 1s
Type "]"
Sleep 3s
Hide
Type "q"
Wait
