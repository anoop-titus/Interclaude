# Project State

## Current Position
Phase 11: TUI/UX Redesign — Plan 01 complete, Plan 02 next

## Accumulated Decisions
- MOSH recommended over SSH (survives roaming, auto-reconnects)
- All 3 transports user-selectable at any time (not failover-only)
- Single .ledger for dedup across all transports
- Only ONE transport active at a time
- 7-stage delivery tracking pipeline
- UUIDv7 for time-sortable message IDs
- Role negotiation: first initiator = master, tie-break by machine_id

## Deferred Issues
- 39 compiler warnings (all "unused") — will reduce as features exercised
- Mouse click offsets are hardcoded (fragile)
- Redis password stored as plaintext in config.toml

## Blockers/Concerns
- None blocking

## Alignment
On track. Core functionality complete. UI/UX polish is next priority.
