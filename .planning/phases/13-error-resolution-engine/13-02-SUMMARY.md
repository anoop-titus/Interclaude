# Phase 13 Plan 02: Error Analysis & Overlay Summary

**Claude API integration for error analysis with overlay popup**

## Performance

- **Duration:** ~6 min
- **Tasks:** 4 (all auto, no checkpoints)
- **Files modified:** 7 (api/mod.rs, api/anthropic.rs, error/analysis.rs, error/mod.rs, tui/error_overlay.rs, tui/mod.rs, app.rs new)

## Accomplishments
- Created `src/api/anthropic.rs` — minimal Anthropic API client (send method for /v1/messages)
- Created `src/error/analysis.rs` — AnalysisResult struct, FixType enum (InSession/OutOfSession), analyze_error() with structured JSON prompt
- Created `src/tui/error_overlay.rs` — centered popup overlay with error summary, fix type, confidence, suggested action, [Y]/[N]/[D] action bar
- Error overlay renders on top of any page (draws after page content)
- Overlay intercepts all keys when active: Y=accept fix, N/Esc=dismiss, D=toggle details
- Auto-trigger: when new error with severity >= Error is captured and API credentials are configured, spawns async analysis
- Analysis result received via mpsc channel, shown as overlay on next frame
- System prompt instructs Claude to classify errors as in_session vs out_of_session with structured JSON response
- Graceful fallback: if Claude returns non-JSON, wraps in default analysis result
- Overlay width capped at 60 cols, centered vertically and horizontally

## Files Modified
- `src/api/mod.rs` — New: API module root
- `src/api/anthropic.rs` — New: Anthropic API client
- `src/error/analysis.rs` — New: AnalysisResult, FixType, analyze_error()
- `src/error/mod.rs` — Added analysis module
- `src/tui/error_overlay.rs` — New: overlay popup rendering
- `src/tui/mod.rs` — Added error_overlay module, overlay rendering in draw(), overlay key handling, analysis trigger in event loop
- `src/app.rs` — Added active_error_overlay, show_error_details fields
- `src/main.rs` — Added `mod api`

## Decisions Made
- Analysis trigger checks error count change each frame — simple but effective
- Only triggers if no overlay already active (prevents spam)
- JSON structured response from Claude — falls back gracefully if model doesn't comply
- Y on overlay stores TODO for correction module (Plan 13-03)
- 35 warnings unchanged

---
*Phase: 13-error-resolution-engine*
*Completed: 2026-03-22*
