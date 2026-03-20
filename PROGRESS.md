# Interclaude - Build Progress

## Project
- **Name**: Interclaude - Cross-Machine Claude Code Bridge
- **Language**: Rust (edition 2024)
- **Location**: /Users/titus/.claude/projects/Interclaude_1773900890/
- **Plan**: /Users/titus/.claude/plans/nested-humming-waffle.md
- **Binary**: target/debug/interclaude

## Architecture Summary
Two Claude Code sessions (Mac + Linux VPS) communicate via 3 selectable transports:
- [1] rsync (file-based, 2-5s latency, most reliable)
- [2] MCP (tool calls over SSH tunnel, ~0.5s)
- [3] Redis Pub/Sub (near-real-time, <0.1s)

Connection: MOSH (recommended) or SSH, user-selectable.
Messages: JSON + UUIDv7, deduplication via .ledger, 7-stage delivery tracking.

## Phase Status

### PHASE 1: Project Scaffold + TUI Shell -- COMPLETE
- Cargo.toml with all deps (ratatui, crossterm, tokio, serde, redis, uuid, chrono, clap, anyhow, dirs)
- .cargo/config.toml with LIBRARY_PATH fix for macOS libiconv
- App state machine: Welcome -> Setup -> Bridge pages
- Welcome page: ASCII logo, dependency preflight (ssh, mosh, autossh, rsync, fswatch, redis-cli, claude)
- Setup page: 10-field form (host, connection type, user, port, key, dir, transport, redis config)
- Bridge page: split outbox/inbox, transport selector, delivery pipeline viewer
- Files: src/main.rs, src/app.rs

### PHASE 2: Setup Page (SSH + Config) -- COMPLETE
- Settings load/save to ~/.interclaude/config.toml (TOML serialization)
- F2: Real MOSH/SSH connection test (checks handshake + remote mosh-server)
- F3: SSH key generation (ed25519)
- Ctrl+S: Save config from any page
- Ctrl+Enter: Save + create local/remote dirs + proceed to bridge
- ConnectionKind enum (Mosh/Ssh) with cycle()
- Settings helpers: ssh_destination(), ssh_args(), expand_path(), local_interclaude_dir()
- Files: src/config/settings.rs, src/config/mod.rs

### PHASE 3: Bridge Core (Connection + Sync) -- COMPLETE
- connection.rs: test_connection(), setup_local_dirs(), setup_remote_dirs(), start_autossh_tunnel(), remote_exec()
- sync.rs: rsync_once() (push/pull), sync_ledger() (bidirectional merge), sync_status()
- watcher.rs: FileWatcher (fswatch Mac / inotifywait Linux) + PollingWatcher fallback
- session.rs: remote_claude_exec(), launch_slave_watcher()
- message.rs: Message struct with UUIDv7, MessageType, MessagePayload variants
- Files: src/bridge/*.rs (5 files)

### PHASE 4: Transport Trait + rsync Pathway -- COMPLETE
- Transport trait: kind(), send(), receive(), health_check()
- TransportSelector: manages all 3 transports, tracks health[3]
- RsyncTransport: write JSON to outbox, rsync push/pull, health via dry-run
- Dedup ledger: .ledger file, is_seen(), mark_seen()
- Status tracker: .status/ directory, 7-stage pipeline, history per msg_id
- Files: src/transport/*.rs (4 files)

### PHASE 5: MCP Pathway -- COMPLETE
- McpTransport implements Transport trait
- JSON-RPC-like protocol over SSH-tunneled TCP (localhost:mcp_port)
- Methods: send_message, receive_messages, health_check
- McpServer: listens on TCP, handles connections, queues messages
- Full audit trail: writes all messages to Inbox/Outbox files
- File: src/transport/mcp_transport.rs

### PHASE 6: Redis Pub/Sub Pathway -- COMPLETE
- RedisTransport implements Transport trait
- Pub/Sub channels: interclaude:{session_id}:{role} for directional messaging
- Background subscriber task with auto-reconnect on failure
- Redis URL builder with optional password support
- Full audit trail: writes all messages to Inbox/Outbox files
- Dependency: futures-lite for async stream processing
- File: src/transport/redis_transport.rs

### PHASE 7: Transport Selector + Switching -- COMPLETE
- BridgeEngine manages all 3 transports through TransportSelector
- Hotkeys [1][2][3] trigger transport switch with switching protocol
- Switching: announce on current -> activate new -> health check -> rollback on fail (10s timeout)
- Independent health checks per transport (15s interval)
- TUI shows all 3 health statuses in real-time
- File: src/bridge/engine.rs

### PHASE 8: Claude Code Session Management -- COMPLETE
- BridgeEngine integrates session.rs for slave launch (Ctrl+L)
- Slave watcher daemon monitors Inbox, processes via claude -p, writes responses
- Status updates flow through engine to TUI (READ, EXECUTING, EXECUTED, REPLYING)
- Heartbeat sender (30s interval) keeps connection alive
- Receive loop polls active transport for incoming messages
- Headless --slave mode: runs engine without TUI, logs events to stderr
- File: src/main.rs (slave mode), src/bridge/engine.rs

### PHASE 9: Bridge TUI (Live Monitoring) -- COMPLETE
- Bridge engine events flow to TUI via mpsc channel
- Real-time message list updates (sent/received)
- Transport health updates displayed live
- Delivery pipeline visualization for selected message
- Ctrl+N: Compose mode with text input, Enter to send, Esc to cancel
- Ctrl+L: Launch slave watcher on remote
- Bridge log shows latest engine activity
- Empty-state messages guide user ("No messages sent yet. Press Ctrl+N to compose.")
- Files: src/tui/mod.rs, src/tui/bridge.rs

### PHASE 10: Role Negotiation (Master/Slave) -- COMPLETE
- Handshake state machine: Idle -> Proposed -> Confirmed(Role)
- First machine to initiate = Master (via proposal message)
- Simultaneous proposal tie-breaking: lexicographic machine_id comparison
- HandshakePayload: proposed_role, machine_id, session_id, protocol_version
- Role swap support via create_role_swap()
- Handshake messages embedded in Command/Response types with __handshake__ marker
- File: src/bridge/handshake.rs

## Scripts
- scripts/slave-watcher.sh: Full watcher daemon for remote (monitors inbox, claude -p, writes responses, updates status+ledger)
- scripts/install-deps.sh: Cross-platform dependency checker/installer

## Key Decisions
- MOSH recommended over SSH (survives roaming, auto-reconnects, UDP-based)
- All 3 transports user-selectable at any time (not failover-only)
- Every transport writes files for audit trail regardless of delivery method
- Single .ledger for dedup across all transports
- Only ONE transport active at a time to prevent duplication
- 7-stage delivery tracking: delivered->read->executing->executed->replying->receiving_reply->received_reply
- UUIDv7 for time-sortable message IDs
- Transport switching: announce->drain->activate->health_check->rollback_on_fail (10s)
- Role negotiation: first initiator = master, tie-break by machine_id

## Build Notes
- macOS linker fix: .cargo/config.toml sets LIBRARY_PATH for libiconv
- Rust 1.93.1, Cargo 1.93.1
- 39 warnings (all "unused" - expected, will reduce as features are fully exercised)
- Binary compiles and runs: interclaude --help, interclaude --slave both work
- Dependencies: ratatui, crossterm, tokio, serde, serde_json, toml, clap, uuid, redis, futures-lite, anyhow, thiserror, chrono, dirs, hostname

## File Manifest
```
src/
  main.rs                         -- Entry point, CLI, --slave headless mode
  app.rs                          -- App state machine, DeliveryStatus, compose mode
  config/
    mod.rs                        -- Exports Settings, ConnectionKind, Role
    settings.rs                   -- Config persistence, SSH helpers, field get/set
  tui/
    mod.rs                        -- TUI loop, input handling, bridge engine lifecycle
    welcome.rs                    -- ASCII logo, dependency preflight
    setup.rs                      -- 10-field form, connection test, key gen
    bridge.rs                     -- Live bridge view with compose, pipeline, log
  transport/
    mod.rs                        -- Transport trait, TransportKind, TransportSelector
    rsync_transport.rs            -- rsync file-based transport
    mcp_transport.rs              -- MCP JSON-RPC transport + McpServer
    redis_transport.rs            -- Redis Pub/Sub transport
    dedup.rs                      -- Deduplication ledger (.ledger)
    status.rs                     -- Delivery status tracker (.status/)
  bridge/
    mod.rs                        -- Bridge module exports
    connection.rs                 -- SSH/MOSH test, dir setup, autossh tunnel
    engine.rs                     -- BridgeEngine: transport lifecycle, send/receive
    handshake.rs                  -- Role negotiation handshake protocol
    message.rs                    -- Message format, UUIDv7, payloads
    session.rs                    -- Remote claude exec, slave watcher launch
    sync.rs                       -- rsync push/pull, ledger sync, status sync
    watcher.rs                    -- FileWatcher (fswatch/inotifywait) + PollingWatcher
scripts/
  slave-watcher.sh                -- Standalone slave watcher (bash)
  install-deps.sh                 -- Cross-platform dependency checker
```
