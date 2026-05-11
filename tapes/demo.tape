Output target/vhs/tui-tracing-demo.gif

Require cargo
Require vhs

Set Shell "bash"
Set FontSize 18
Set Width 1400
Set Height 900
Set Padding 16
Set Framerate 24
Set PlaybackSpeed 1
Set TypingSpeed 25ms
Set Theme "Catppuccin Mocha"

Hide
Type "rm -f trace.log"
Enter
Wait
Show

Type "cargo run --example demo"
Enter
Wait+Screen@90s /tui-tracing demo/
Wait+Screen@10s /tail .* visible/
Sleep 1s

Type "d"
Wait+Screen@10s /d level TRACE\+/
Type "d"
Wait+Screen@10s /d level ERROR\+/
Up 5
Wait+Screen@10s /scrollback/
Down 2
Sleep 1s
Down 10
Wait+Screen@10s /tail .* visible/
Type "q"
Wait
