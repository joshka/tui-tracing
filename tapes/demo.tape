Output target/vhs/tui-tracing-demo.gif

Require cargo
Require vhs

Set Width 1200
Set Height 760
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
Sleep 6s

Type "d"
Sleep 3s
Type "d"
Sleep 3s
Up 5
Sleep 4s
Down 2
Sleep 2s
Down 10
Sleep 4s
Hide
Type "q"
Wait
