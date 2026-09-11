# ferrum-tui demo — recorded with VHS (https://github.com/charmbracelet/vhs).
#
# Synthetic data only: `--dry-run` performs no HTTP calls (bins/ferrum-tui/README.md).
#
# Reproduce from the repository root:
#   cargo build --release --bin ferrum-tui
#   vhs assets/tui-demo.tape
# Tested with VHS v0.11.0.

Output assets/tui-demo.gif

Require ./target/release/ferrum-tui

Set Shell bash
Set FontFamily "DejaVu Sans Mono"
Set FontSize 14
Set Width 1160
Set Height 610
Set Padding 24
Set Framerate 15
Set TypingSpeed 40ms
# Dark palette tokens from assets/README.md, mapped to the ANSI slots the TUI
# uses: blue #7aa2f7 borders, violet #bb9af7 accents, rust #d97757 errors,
# mute #d8dce6 text, muted #8b92a8 secondary text, dim #565f89 tertiary text.
Set Theme '{"name":"ferrumgate-dark","background":"#0f1115","foreground":"#d8dce6","selection":"#2d3344","cursor":"#bb9af7","cursorAccent":"#0f1115","black":"#0f1115","brightBlack":"#565f89","red":"#d97757","brightRed":"#d97757","green":"#9ece6a","brightGreen":"#9ece6a","yellow":"#e0af68","brightYellow":"#e0af68","blue":"#7aa2f7","brightBlue":"#7aa2f7","magenta":"#bb9af7","brightMagenta":"#bb9af7","cyan":"#bb9af7","brightCyan":"#bb9af7","white":"#8b92a8","brightWhite":"#d8dce6"}'

# Start from a clean prompt; these steps run before recording begins.
Hide
Type "export PS1='$ '"
Enter
Type "clear"
Enter
Show

# Launch the dashboard against synthetic dry-run data.
Type "./target/release/ferrum-tui --dry-run"
Enter
Sleep 3s

# Approvals: select a row, then open the detail overlay.
Type "a"
Sleep 2s
Type "j"
Sleep 1.2s
Enter
Sleep 2.8s
Escape
Sleep 500ms

# Metrics: filter the summary live by metric name.
Type "3"
Sleep 2s
Type "/"
Sleep 300ms
Type "pg_pool"
Sleep 1.3s
Enter
Sleep 2s
