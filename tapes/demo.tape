Output target/vhs/tui-tracing-demo.gif

Require cargo
Require vhs

Set Width 1200
Set Height 1024
Set TypingSpeed 25ms
Set Theme "Aardvark Blue"

Hide
Type "rm -f trace.log"
Enter
Wait
Type "cargo run --example demo"
Enter
Sleep 4s
Show
Sleep 3s

Type "g"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 5s
Hide
Type "q"
Wait
