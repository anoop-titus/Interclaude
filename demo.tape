# Interclaude Demo - VHS Tape
# Generate: vhs demo.tape
# Requires: https://github.com/charmbracelet/vhs

Output assets/demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1200
Set Height 600
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

# Title
Type "# Interclaude - Cross-Machine Claude Code Bridge"
Enter
Enter
Sleep 1s

# Show config example
Type "cat << 'EOF'"
Enter
Type "# ~/.interclaude/config.toml"
Enter
Type "remote_host = \"vps.example.com\""
Enter
Type "ssh_user = \"deploy\""
Enter
Type "ssh_port = 22"
Enter
Type "key_path = \"~/.ssh/id_ed25519\""
Enter
Type "remote_dir = \"~/Interclaude\""
Enter
Type "active_transport = \"rsync\""
Enter
Type "EOF"
Enter
Sleep 3s

Enter

# Show directory structure
Type "echo '~/Interclaude/'"
Enter
Type "echo '├── Master/'"
Enter
Type "echo '│   ├── Inbox/    ← Responses arrive here'"
Enter
Type "echo '│   └── Outbox/   ← Commands sent from here'"
Enter
Type "echo '├── Slave/'"
Enter
Type "echo '│   ├── Inbox/    ← Commands received here'"
Enter
Type "echo '│   └── Outbox/   ← Responses sent from here'"
Enter
Type "echo '└── .status/      ← Health & delivery tracking'"
Enter
Sleep 3s

Enter

# Show message flow
Type "echo 'Pipeline: SENT → READ → RUNNING → STREAMING → COMPLETE'"
Enter
Sleep 2s
Type "echo 'Responses stream back line-by-line in real-time.'"
Enter
Sleep 2s

Enter

# Launch
Type "interclaude"
Enter
Sleep 5s

# Welcome page auto-advances, go to Setup
Enter
Sleep 2s

# Quit
Ctrl+q
Sleep 1s
