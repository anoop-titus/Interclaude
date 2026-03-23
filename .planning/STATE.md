# Project State

## Current Position
Phase 12: Word Wrap + Access Portal — COMPLETE (all 3 plans executed)

## Accumulated Decisions
- MOSH recommended over SSH (survives roaming, auto-reconnects)
- All 3 transports user-selectable at any time (not failover-only)
- Single .ledger for dedup across all transports
- Only ONE transport active at a time
- 7-stage delivery tracking pipeline with human-readable labels
- UUIDv7 for time-sortable message IDs
- Role negotiation: first initiator = master, tie-break by machine_id
- Setup form: progressive disclosure hides Redis fields when not selected
- Nav bars simplified to essential keys; other shortcuts discoverable via tutorial/help
- Esc = back (never quit); Ctrl+Q/Ctrl+C = quit
- Transport recommendation is passive (suggest, never auto-switch)
- Auto-collapse panels on small terminals (< 20 rows)
- Box-constrained text selection NOT possible (terminal limitation, not app)
- Default access mode: ApiKey (most common for CLI users)
- Default model: Sonnet 4.6 (recommended balance of speed/capability)
- Credential encryption: AES-256-GCM with machine-bound key derivation
- Credentials not portable between machines (by design)

## Deferred Issues
- 35 compiler warnings (all "unused" — will reduce as features exercised)
- Mouse click offsets approximate (section headers shift rows)
- Redis password stored as plaintext in config.toml (masked in UI only)
- F6 pipeline toggle deferred (pipeline inside status panel, F5 covers both)
- is_configured and CredentialConfig import unused (will be used by ERE in Phase 13)

## Blockers/Concerns
- None blocking

## Alignment
Phase 12 complete. Word wrapping, Access Portal, credential storage all working. Ready for Phase 13 (ERE).
