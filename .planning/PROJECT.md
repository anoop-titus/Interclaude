# Interclaude - Cross-Machine Claude Code Bridge

## Vision
Enable two Claude Code sessions (Mac + Linux VPS) to communicate seamlessly via selectable transports (rsync, MCP, Redis Pub/Sub) with a polished ratatui TUI.

## Stack
- **Language**: Rust (edition 2024)
- **TUI**: ratatui 0.29 + crossterm 0.28
- **Async**: tokio (full features)
- **Serialization**: serde + serde_json + toml
- **Transports**: rsync (file-based), MCP (JSON-RPC over SSH tunnel), Redis Pub/Sub
- **Connection**: SSH/MOSH with autossh tunnels

## Current State
- 10 phases complete (scaffold through role negotiation)
- All 3 transports implemented and functional
- TUI has 3 pages: Welcome, Setup, Bridge
- Binary compiles and runs on macOS (Rust 1.93.1)

## Priorities
1. Seamlessness — zero-friction flow
2. User-friendliness — intuitive without docs
3. Universal installability — works on any terminal
4. Networking reliability — connection state clarity
5. Security — credential handling, key management
