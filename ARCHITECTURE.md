# Interclaude Architecture

> Cross-machine Claude Code bridge — 8,835 lines of Rust

## System Overview

```
 LOCAL MACHINE (Master)                        REMOTE MACHINE (Slave)
┌────────────────────────────────────┐        ┌──────────────────────────┐
│                                    │        │                          │
│  ┌──────────┐    ┌──────────────┐  │  SSH   │  ┌────────────────────┐  │
│  │   TUI    │───►│ BridgeEngine │──┼────────┼─►│  claude -p '...'   │  │
│  │ (ratatui)│◄───│  (engine.rs) │◄─┼────────┼──│  (streaming stdout)│  │
│  └──────────┘    └──────┬───────┘  │        │  └────────────────────┘  │
│                         │          │        │                          │
│                  ┌──────┴───────┐  │        │  ┌────────────────────┐  │
│                  │  Transport   │──┼────────┼─►│ ~/Interclaude/     │  │
│                  │  Selector    │  │ rsync/ │  │  Master/Inbox/     │  │
│                  │              │  │ Redis/ │  │  Master/Outbox/    │  │
│                  │ rsync│MCP│Red│  │  MCP   │  │  Slave/Inbox/      │  │
│                  └──────────────┘  │        │  │  Slave/Outbox/     │  │
│                                    │        │  └────────────────────┘  │
└────────────────────────────────────┘        └──────────────────────────┘
```

## Module Map

### Entry Point

**`src/main.rs`** (200 lines)
- CLI arg parsing: `--slave` flag for remote mode
- Initializes `App` state, launches TUI via `tui::run()`
- Slave mode: `run_slave()` — listens for commands and executes locally

### Application State

**`src/app.rs`** (607 lines)
- `App` struct: central state for all TUI pages
- `Page` enum: `Welcome → Setup → AccessPortal → Bridge`
- `DeliveryStatus` enum: 11 states including `Streaming` (pipeline visualization)
- `MessageEntry`: display model for outbox/inbox messages
- `SetupField`: form field navigation with transport-aware skip logic
- `BridgeFocus`: which Bridge panel has keyboard focus

### TUI Layer (`src/tui/`)

**`src/tui/mod.rs`** (1,316 lines) — Main event loop
- `run()`: terminal setup, raw mode, alternate screen
- `run_loop()`: 100ms poll loop, processes bridge events, keyboard, mouse
- `execute_action()`: dispatches `InputAction` variants (StartBridge, SendCommand, Activate, etc.)
- `process_bridge_event()`: handles 12 event types including `StreamChunk`, `StreamComplete`
- Session cleanup on exit: wipes local + remote Inbox/Outbox contents
- Fresh directory creation on every bridge start

**`src/tui/welcome.rs`** (251 lines) — Dependency check page
- Async checks for: ssh, rsync, mosh, redis-cli, autossh, claude
- Required vs Optional grouping with color-coded status
- Auto-advance timer (2s) when all required deps met

**`src/tui/setup.rs`** (400 lines) — Configuration form
- Progressive disclosure: Redis fields hidden unless Redis transport selected
- Section headers: Connection, Transport
- Inline validation, one-click Activate sequence (Test → Save → Push → Bridge)

**`src/tui/access_portal.rs`** (207 lines) — Credential management
- API Key / OAuth mode selection
- Key masking (shows `sk-ant-` prefix + bullets)
- Ctrl+V triggers async API validation

**`src/tui/bridge.rs`** (727 lines) — Main bridge interface
- Three-panel layout: Outbox (left), Inbox (right), Input (bottom)
- Delivery pipeline visualization with animated stages
- Collapsible status/pipeline panels (F5)
- Streaming indicator: pulsing cyan "STREAMING" stage
- Help overlay (Ctrl+H)
- Autoscroll-aware rendering with manual scroll override

**`src/tui/status_bar.rs`** (152 lines) — Global top bar
- Tab navigation (Welcome/Setup/Access/Bridge)
- ERE status indicator, connection status, session status
- Width-aware truncation

**`src/tui/error_overlay.rs`** (162 lines) — ERE analysis popup
- Renders `AnalysisResult` as scrollable overlay
- Root cause, suggestion, confidence, affected files

### Bridge Layer (`src/bridge/`)

**`src/bridge/engine.rs`** (893 lines) — Core orchestrator
- `BridgeEngine`: holds all transport instances, event channel, settings
- `BridgeEvent` enum (12 variants): MessageSent/Received, StreamChunk/Complete, PingResult, HealthUpdate, ConnectionStatus, StatusUpdate, Log, TransportSwitched, RoleConfirmed, CommandReceived
- Background tasks (all `tokio::spawn`):
  - `start_health_monitor()`: 15s interval, checks all 3 transports
  - `start_receive_loop()`: polls active transport + rsync, processes messages, handles handshake/ping/pong
  - `start_heartbeat()`: periodic ping for RTT measurement
  - `start_redis_subscriber_if_active()`: Redis pub/sub listener
- `execute_remote_command()`: streaming execution via SSH
  - Creates mpsc channel for chunks
  - Spawns `remote_claude_exec_streaming()` in separate task
  - Forwards chunks as `BridgeEvent::StreamChunk`
  - Emits `BridgeEvent::StreamComplete` on finish
  - Timeout wraps entire operation

**`src/bridge/session.rs`** (150 lines) — Remote execution
- `remote_claude_exec()`: blocking SSH → `claude -p` (legacy, unused)
- `remote_claude_exec_streaming()`: streaming variant
  - `Command::spawn()` with piped stdout + `-tt` for PTY
  - `BufReader` reads line-by-line via `AsyncBufReadExt`
  - Sends accumulated text through mpsc channel
  - Returns `ClaudeExecResult` with full output on completion
- `launch_slave_watcher()`: starts `interclaude --slave` on remote

**`src/bridge/connection.rs`** (413 lines) — SSH operations
- `test_connection()`: SSH or MOSH connectivity test
- `setup_remote_dirs()`: creates Interclaude directory structure via SSH
- `setup_local_dirs()`: creates local directory structure
- `cleanup_remote_contents()`: SSH `rm -f` all Inbox/Outbox files (exit cleanup)
- `cleanup_local_contents()`: local file cleanup
- `start_autossh_tunnel()`: persistent SSH tunnel management
- `push_install_slave()`: SCP binary to remote `~/.local/bin/`

**`src/bridge/message.rs`** (181 lines) — Wire protocol
- `Message` struct: JSON-serialized, UUID v7 IDs, sequence numbers
- 8 message types: Command, Response, Status, Error, Heartbeat, TransportSwitch, Ping, Pong
- Typed payloads: `CommandPayload`, `ResponsePayload`, `PingPayload`, etc.
- File naming: `{timestamp}_{sequence}_{type}.json`

**`src/bridge/handshake.rs`** (206 lines) — Role negotiation
- State machine: `Idle → SentOffer → Confirmed`
- Master sends offer, slave accepts, roles confirmed
- Handshake messages encoded as Command type with magic prefix

**`src/bridge/sync.rs`** (203 lines) — rsync operations
- `rsync_once()`: single push or pull between local and remote
- Bidirectional: `SyncDirection::Push` / `Pull`
- Maps Master/Outbox → Slave/Inbox and vice versa based on role

**`src/bridge/watcher.rs`** (155 lines) — File change detection
- `FileWatcher`: fswatch/inotifywait-based watching
- `PollingWatcher`: fallback interval-based checking
- Currently unused — receive loop polls instead

### Transport Layer (`src/transport/`)

**`src/transport/mod.rs`** (100 lines) — Abstraction
- `Transport` trait: `send()`, `receive()`, `health_check()`
- `TransportKind` enum: Rsync, Mcp, Redis
- `TransportSelector`: active transport + health tracking

**`src/transport/rsync_transport.rs`** (124 lines) — File-based backbone
- Write JSON to outbox → rsync push
- rsync pull → read JSON from inbox → delete processed files
- Health check: rsync dry-run

**`src/transport/mcp_transport.rs`** (263 lines) — Model Context Protocol
- HTTP-based JSON-RPC transport
- Built-in MCP server for receiving
- Tunneled through autossh

**`src/transport/redis_transport.rs`** (213 lines) — Redis pub/sub
- Publish messages to channel, subscribe for incoming
- Tunneled through autossh to remote Redis
- Subscriber runs as background task

**`src/transport/dedup.rs`** (52 lines) — Message deduplication
- In-memory + file-backed ledger
- Prevents re-processing of already-seen messages

**`src/transport/status.rs`** (80 lines) — Delivery tracking
- File-backed status entries per message ID
- Tracks delivery pipeline progression

### Configuration (`src/config/`)

**`src/config/settings.rs`** (204 lines)
- `Settings` struct: TOML-serialized to `~/.interclaude/config.toml`
- SSH args builder (skips `~/.ssh/config` to avoid Colima/OrbStack issues)
- Path expansion (`~/` → home dir)
- Form field get/set helpers

**`src/config/credentials.rs`** (192 lines)
- Encrypted API key storage using ring HKDF + AES-GCM
- Machine-specific key derivation (hostname + username)
- `CredentialConfig`: access_mode, model, encrypted_key

### Error Resolution Engine (`src/error/`)

**`src/error/analysis.rs`** — Anthropic API error analysis
- Sends error context to Claude API for diagnosis
- Returns structured `AnalysisResult` (root cause, suggestion, confidence)

**`src/error/correction.rs`** — Auto-correction pipeline
- 3-stage: detect → analyze → suggest fix

**`src/error/logging.rs`** — Error store
- `ErrorStore`: in-memory ring buffer with severity levels
- Thread-safe (Arc-based)

**`src/error/pending.rs`** — Pending fix queue

### Logging

**`src/logging.rs`** — File-based debug log
- Writes to `~/.interclaude/debug.log`
- Used throughout for non-TUI diagnostic output

## Data Flow: Sending a Command

```
User types "Explain Builder pattern"
         │
         ▼
┌─ InputAction::SendCommand ─────────────────────────────┐
│                                                         │
│  1. engine.send_command(task)                           │
│     → Creates Message(Command) with UUID v7             │
│     → Writes JSON to Master/Outbox/                     │
│     → rsync push to remote                              │
│     → Emits BridgeEvent::MessageSent                    │
│     → Returns msg_id                                    │
│                                                         │
│  2. engine.execute_remote_command(task, msg_id)          │
│     → Emits StatusUpdate(msg_id, Executing)              │
│     → Spawns remote_claude_exec_streaming()              │
│       → ssh -tt user@host "cd ~/Interclaude &&           │
│          claude -p 'Explain Builder pattern'              │
│          < /dev/null 2>/dev/null"                         │
│     → BufReader reads stdout line-by-line                │
│     → Each line → StreamChunk(resp_id, accumulated)      │
│       → TUI updates inbox entry in-place                 │
│     → EOF → StreamComplete(resp_id, final_text)          │
│     → Emits StatusUpdate(msg_id, ReceivedReply)          │
│                                                         │
│  Pipeline visualization:                                │
│  SENT → READ → RUNNING → STREAMING → COMPLETE           │
└─────────────────────────────────────────────────────────┘
```

## Data Flow: Session Lifecycle

```
┌─ Activate (Setup page) ──────────────────┐
│  1. test_connection() — SSH/MOSH check    │
│  2. settings.save() — persist to TOML     │
│  3. push_install_slave() — SCP binary     │
│  4. setup_local_dirs() + cleanup          │
│  5. setup_remote_dirs() + cleanup         │
│  6. Navigate to Bridge page               │
│  7. StartBridge action triggered          │
└───────────────┬──────────────────────────┘
                ▼
┌─ StartBridge ────────────────────────────┐
│  1. BridgeEngine::new(settings, tx)       │
│  2. start_tunnels() — autossh if needed   │
│  3. start_health_monitor() — 15s loop     │
│  4. start_receive_loop() — poll loop      │
│  5. start_heartbeat() — ping/pong         │
│  6. send_handshake() — role negotiation   │
│  7. Emit RoleConfirmed + Connected        │
└───────────────┬──────────────────────────┘
                ▼
┌─ Active Session ─────────────────────────┐
│  User sends commands, receives responses  │
│  Health monitor tracks transport status   │
│  Ping/pong measures RTT                   │
│  Transport can be switched live (1/2/3)   │
└───────────────┬──────────────────────────┘
                ▼
┌─ Exit (Ctrl+Q) ─────────────────────────┐
│  1. app.running = false                   │
│  2. cleanup_local_contents()              │
│  3. cleanup_remote_contents() via SSH     │
│  4. Terminal teardown                     │
│  5. Tunnel handles dropped (kill_on_drop) │
└──────────────────────────────────────────┘
```

## Message Protocol

All messages are JSON files with this structure:

```
{
  "msg_id": "01968d3f-...",       // UUID v7 (time-ordered)
  "msg_type": "command",           // command|response|status|ping|pong|...
  "timestamp": "2026-03-23T...",   // ISO 8601
  "sequence": 42,                  // Monotonic counter
  "sender_role": "master",         // master|slave
  "transport_used": "rsync",       // rsync|MCP|Redis
  "payload": { ... }               // Type-specific payload
}
```

File naming: `{YYYYMMDD_HHMMSS}_{sequence}_{type}.json`

## Transport Architecture

```
┌─────────────────────────────────────────────┐
│              TransportSelector              │
│         active: rsync | MCP | Redis         │
├─────────────┬──────────────┬────────────────┤
│   rsync     │     MCP      │    Redis       │
│ (backbone)  │  (overlay)   │  (overlay)     │
├─────────────┼──────────────┼────────────────┤
│ File I/O +  │ HTTP JSON-   │ Pub/Sub via    │
│ rsync over  │ RPC over     │ tunneled       │
│ SSH         │ autossh      │ autossh        │
│             │ tunnel       │ tunnel         │
└─────────────┴──────────────┴────────────────┘

- rsync is ALWAYS active as backbone (receive loop always polls it)
- MCP and Redis are overlays for faster pickup
- Messages sent on active transport + rsync (dual-write)
- Dedup ledger prevents double-processing
```

## Key Design Decisions

1. **Direct SSH execution over slave BridgeEngine** — The slave was removed in Phase 15. Master SSHs directly to remote and runs `claude -p`. Simpler, more reliable, avoids self-referential config issues.

2. **Streaming via PTY allocation** — `-tt` flag forces pseudo-terminal on SSH, which makes Claude's output line-buffered instead of block-buffered. This enables real-time streaming.

3. **rsync as backbone** — Always available since it uses SSH (no extra infrastructure). MCP and Redis are optional accelerators.

4. **File-based message protocol** — JSON files in Inbox/Outbox directories. Simple, debuggable, survives transport failures. Processed files are deleted to prevent unbounded growth.

5. **Session cleanup on exit** — All message files wiped on both machines. Prevents stale data from confusing the next session.

6. **Credential encryption** — API keys encrypted with machine-specific HKDF derivation. Never stored in plaintext config.
