# The README demo: `vhs docs/demo/demo.tape` from the repository root
# (after `cargo build --release`). Renders docs/site/img/demo.gif.

Output docs/site/img/demo.gif

Require git

Set Shell bash
Set Theme "OneDark"
Set FontSize 16
Set Width 1200
Set Height 660
Set Padding 18
Set TypingSpeed 35ms
Set Framerate 24

Hide
Type "TACKLINE_BIN=${TACKLINE_BIN:-$PWD/target/release} source docs/demo/setup.sh" Enter
Sleep 1s
Show

Type "tack --theme TwoDark" Sleep 200ms Enter
Sleep 1.2s

# Find a file.
Ctrl+P Sleep 400ms
Type "matcher" Sleep 500ms Enter
Sleep 900ms

# Select the loop and write what the agent should do there.
Type "jjj" Sleep 300ms
Type "V" Sleep 200ms
Type@120ms "jjjj" Sleep 500ms
Type "c" Sleep 500ms
# The first note asks for a session name.
Type "Search speedup" Sleep 300ms Enter
Sleep 500ms
Type "Use an iterator chain instead of the loop" Sleep 400ms
Enter
Sleep 1.2s

# The session: every note with its code, in prompt order.
Type "3" Sleep 1.8s
Type "q" Sleep 300ms

# Hand it to any agent: it's plain Markdown.
Type "tack export" Sleep 200ms Enter
Sleep 2.8s
