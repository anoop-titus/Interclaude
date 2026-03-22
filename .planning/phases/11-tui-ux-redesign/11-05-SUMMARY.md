# Phase 11 Plan 05: Collapsible Panels & Transport Recommendation Summary

**F5 panel toggle, auto-collapse on small terminals, passive transport recommendation**

## Performance

- **Duration:** ~8 min (batched with Plans 03 and 04)
- **Tasks:** 3 (2 auto + 1 checkpoint, checkpoint auto-approved in autonomous mode)
- **Files modified:** 3 (bridge.rs, app.rs, mod.rs)

## Accomplishments
- F5 toggles status panel visibility on Bridge page
- show_status_panel and show_pipeline_panel booleans in App (default: true)
- Auto-collapse: both panels hidden when terminal height < 20 rows
- When panels hidden, messages get full remaining vertical space
- Nav bar shows F5 toggle state: "Status[ON]" / "Status[OFF]"
- Transport recommendation calculated from health checks on every HealthUpdate event
- Priority order: Redis (lowest latency) > MCP (low latency) > rsync (most reliable)
- Recommendation shown as subtle yellow hint in transport header: "Suggested: Redis (lowest latency)"
- No auto-switching — user decides (respects "all 3 transports user-selectable" decision)
- Recommendation clears when current transport is already the best option

## Files Modified
- `src/app.rs` — Added show_status_panel, show_pipeline_panel, transport_recommendation fields, update_transport_recommendation() method, session_duration() method
- `src/tui/bridge.rs` — Conditional status panel rendering, transport recommendation hint in header, F5 toggle state in nav
- `src/tui/mod.rs` — F5 keybinding, transport recommendation recalc on HealthUpdate events

## Decisions Made
- F6 for pipeline toggle deferred — pipeline is inside the status panel, so F5 covers both
- Auto-collapse threshold: 20 rows (standard minimum terminal height)
- Recommendation uses simple priority ordering, not latency measurement (health is binary)
- Warnings reduced from 35 to 32 across all 3 plans

---
*Phase: 11-tui-ux-redesign*
*Completed: 2026-03-22*
