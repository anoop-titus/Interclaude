# Interclaude Demo - VHS Tape
# Generate: vhs demo.tape
# Requires: https://github.com/charmbracelet/vhs

Output assets/demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 700
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

# Launch Interclaude
Type "./target/release/interclaude"
Enter

# Welcome page — dependency checks animate
Sleep 6s

# Advance to Setup
Enter
Sleep 2s

# Show the setup form with demo values
Sleep 4s

# Quit
Ctrl+q
Sleep 1s
