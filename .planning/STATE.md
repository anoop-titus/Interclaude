# Project State

## Current Position
Phase 11: TUI/UX Redesign — Plan 02 complete, Plan 03 next

## Accumulated Decisions
- MOSH recommended over SSH (survives roaming, auto-reconnects)
- All 3 transports user-selectable at any time (not failover-only)
- Single .ledger for dedup across all transports
- Only ONE transport active at a time
- 7-stage delivery tracking pipeline
- UUIDv7 for time-sortable message IDs
- Role negotiation: first initiator = master, tie-break by machine_id
- Setup form: progressive disclosure hides Redis fields when not selected
- Nav bars simplified to essential keys only; other shortcuts still work

## Deferred Issues
- 35 compiler warnings (all "unused") — will reduce as features exercised
- Mouse click offsets are approximate (section headers shift rows)
- Redis password stored as plaintext in config.toml (masked in UI only)

## Blockers/Concerns
- None blocking

## Alignment
On track. Welcome + Setup pages redesigned. Bridge page next.
