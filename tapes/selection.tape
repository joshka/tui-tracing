Output target/vhs/tui-tracing-selection.gif

Require cargo
Require vhs

Set Width 1200
Set Height 760
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

Type "j"
Sleep 1s
Type "j"
Sleep 1s
Type "k"
Sleep 1s
Escape
Sleep 1s
Type "j"
Sleep 4s
Hide
Type "q"
Wait
