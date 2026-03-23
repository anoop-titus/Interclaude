<p align="center">
  <pre align="center">
  ╦╔╗╔╔╦╗╔═╗╦═╗╔═╗╦  ╔═╗╦ ╦╔╦╗╔═╗
 ║║║║ ║ ║╣ ╠╦╝║  ║  ╠═╣║ ║ ║║║╣
  ╩╝╚╝ ╩ ╚═╝╩╚═╚═╝╩═╝╩ ╩╚═╝═╩╝╚═╝
  </pre>
  <strong>Bridge Claude Code across machines. One terminal. Zero friction.</strong>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2024_Edition-orange?logo=rust" alt="Rust"></a>
  <a href="https://github.com/anoop-titus/Interclaude/blob/master/LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License: MIT"></a>
  <a href="https://github.com/ratatui/ratatui"><img src="https://img.shields.io/badge/TUI-ratatui_0.29-purple" alt="ratatui"></a>
  <a href="https://github.com/anoop-titus/Interclaude"><img src="https://img.shields.io/badge/status-alpha-yellow" alt="Status: Alpha"></a>
</p>

---

![Demo](assets/demo.gif)

## Why Interclaude?

You have Claude Code on your laptop. You also have a beefy remote server — maybe a VPS, a GPU box, or a dev machine in the cloud. Wouldn't it be great to send tasks to Claude on that remote machine and watch the response stream back in real-time, all from your local terminal?

**Interclaude** makes that possible. It's a terminal bridge that connects two machines over SSH, lets you fire prompts to a remote Claude Code instance, and streams the response back to your screen as it's being generated. No web UI. No port forwarding. No Docker. Just SSH and a single binary.

## Features

- **Streaming Responses** — Watch Claude think in real-time. Responses appear line-by-line as they're generated on the remote machine, not after a 10-second wait
- **Multi-Transport Messaging** — Three transport backends: **rsync** (file-based, always works), **MCP** (Model Context Protocol), and **Redis** pub/sub. Rsync is the backbone; others are overlay accelerators
- **Beautiful Terminal UI** — Built with ratatui. Four-page guided flow, delivery pipeline visualization, transport health indicators, ping RTT display, and animated status updates
- **Error Resolution Engine (ERE)** — Automatic error analysis powered by the Anthropic API. When something breaks, Interclaude diagnoses the issue and suggests fixes through a 3-stage pipeline
- **Encrypted Credentials** — API keys are encrypted at rest using machine-specific key derivation (ring/HKDF). Never stored in plaintext
- **Auto Session Cleanup** — All message files are wiped from both local and remote machines on exit. Every session starts fresh with clean directories
- **Connection Resilience** — Supports both SSH and MOSH, autossh tunnels for persistent connections, automatic reconnection, and health monitoring

## Quick Start

### Prerequisites

| Dependency | Required | Purpose |
|-----------|----------|---------|
| `ssh` | Yes | Remote connection |
| `rsync` | Yes | File-based transport |
| `claude` CLI | Yes (remote) | Runs prompts on the remote machine |
| `mosh` | No | UDP-based resilient connection |
| `redis-server` | No | Redis pub/sub transport |
| `autossh` | No | Persistent SSH tunnels |

### Install

```bash
git clone https://github.com/anoop-titus/Interclaude.git
cd Interclaude
cargo build --release

# Copy to your PATH
cp target/release/interclaude ~/.local/bin/
```

### Run

```bash
# Launch the TUI (master mode)
interclaude

# The setup wizard will guide you through:
# 1. Remote host / SSH configuration
# 2. Transport selection
# 3. API credentials (optional, for ERE)
# 4. One-click activation: Test → Push → Bridge
```

## How It Works

```
 LOCAL MACHINE                           REMOTE MACHINE
┌─────────────────────┐    SSH pipe     ┌─────────────────────┐
│                     │                │                     │
│  ┌───────────────┐  │   streaming    │  ┌───────────────┐  │
│  │  Interclaude  │──┼───────────────►│  │  claude -p     │  │
│  │  TUI (master) │◄─┼───────────────┤  │  (remote CLI)  │  │
│  └───────────────┘  │  line-by-line  │  └───────────────┘  │
│         │           │                │                     │
│    ┌────┴────┐      │    rsync /     │    ┌──────────┐     │
│    │Transport│◄─────┼── Redis/MCP ──►│    │ Inbox /  │     │
│    │ Layer   │      │                │    │ Outbox   │     │
│    └─────────┘      │                │    └──────────┘     │
└─────────────────────┘                └─────────────────────┘
```

1. **You type a prompt** in the Bridge page input bar
2. **Interclaude SSHs** to the remote and runs `claude -p '<your prompt>'`
3. **Output streams back** line-by-line through the SSH pipe into your inbox panel
4. **Pipeline status** updates in real-time: `SENT → READ → RUNNING → STREAMING → COMPLETE`
5. **On exit**, all message files are cleaned up on both machines

## Keyboard Shortcuts

### Bridge Page

| Key | Action |
|-----|--------|
| `Enter` | Send task to remote Claude |
| `Tab` | Cycle focus: Outbox → Inbox → Input |
| `Up/Down` | Scroll focused message list |
| `1` / `2` / `3` | Switch transport: rsync / MCP / Redis |
| `F5` | Toggle status panel |
| `Ctrl+L` | Launch slave on remote |
| `Ctrl+H` | Toggle help overlay |
| `Esc` | Back to Setup / dismiss overlay |
| `Ctrl+Q` | Quit (cleans up session) |

### Global

| Key | Action |
|-----|--------|
| `Ctrl+Q` / `Ctrl+C` | Quit application |
| `Ctrl+S` | Save configuration (Setup page) |
| `Mouse scroll` | Scroll message panels |

## Architecture

```
src/
├── main.rs              # Entry point, CLI args (--slave mode)
├── app.rs               # Application state, page management
├── logging.rs           # File-based debug logging
├── tui/
│   ├── welcome.rs       # Dependency checking page
│   ├── setup.rs         # SSH/transport configuration
│   ├── access_portal.rs # API credential management
│   ├── bridge.rs        # Main bridge interface
│   ├── status_bar.rs    # Global status bar with ERE indicator
│   └── error_overlay.rs # Error analysis popup
├── bridge/
│   ├── engine.rs        # Core bridge engine, event system
│   ├── session.rs       # SSH session + streaming exec
│   ├── connection.rs    # Connection testing, dir setup, cleanup
│   ├── message.rs       # Message protocol (Command, Response, Ping...)
│   ├── handshake.rs     # Role negotiation protocol
│   ├── sync.rs          # rsync push/pull operations
│   └── watcher.rs       # File system change detection
├── transport/
│   ├── rsync_transport.rs  # File-based transport over SSH
│   ├── mcp_transport.rs    # Model Context Protocol transport
│   ├── redis_transport.rs  # Redis pub/sub transport
│   ├── dedup.rs            # Message deduplication ledger
│   └── status.rs           # Delivery status tracking
├── config/
│   ├── settings.rs      # TOML config management
│   └── credentials.rs   # Encrypted credential storage
├── error/
│   ├── analysis.rs      # Anthropic API error analysis
│   ├── correction.rs    # Auto-correction engine
│   ├── logging.rs       # Error store with severity levels
│   └── pending.rs       # Pending fix queue
└── api/
    └── anthropic.rs     # Anthropic API client
```

## Configuration

Settings are stored at `~/.interclaude/config.toml`:

```toml
remote_host = "your-server.example.com"
ssh_user = "deploy"
ssh_port = 22
key_path = "~/.ssh/id_ed25519"
remote_dir = "~/Interclaude"
local_dir = "~/Interclaude"
sync_interval_secs = 2
role = "master"
connection = "ssh"
active_transport = "rsync"
mcp_port = 9876
message_timeout_secs = 120

[redis]
host = "127.0.0.1"
port = 6379
password = ""
```

## Contributing

Contributions are welcome! This project is in active development.

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Built with Rust, ratatui, and a lot of SSH tunnels.<br>
  <sub>Powered by Claude Code</sub>
</p>
