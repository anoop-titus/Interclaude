# Interclaude Roadmap

## Milestone 1: Core Bridge (Phases 1-10) — COMPLETE
All phases delivered. See PROGRESS.md for details.

## Milestone 2: UI/UX Polish (Phase 11+)

### Phase 11: TUI/UX Redesign
**Goal**: Redesign all 3 TUI pages for seamlessness, usability, and visual polish
**Research**: Unlikely (pure internal ratatui work, patterns exist in codebase)
**Scope**: Large — 3 pages + global elements, split into multiple plans

**Sub-goals**:
- Welcome: Required vs optional deps, loading spinner, auto-advance
- Setup: Progressive disclosure, field grouping, password masking, validation
- Bridge: Input clarity, message scrolling, collapsible panels, simplified pipeline
- Global: Status bar improvements, reconnection visibility, key binding consistency

**Plans**: 11-01 through 11-05 — ALL COMPLETE

### Phase 12: Word Wrap + Access Portal
**Goal**: Universal word wrapping across all TUI sections + new Access Portal tab for API credentials
**Research**: Unlikely (ratatui wrap patterns exist, Anthropic API well-documented)
**Scope**: Medium — 3 plans

**Sub-goals**:
- Plan 1: Audit and fix word wrapping in all widgets across all pages
- Plan 2: New Access Portal page with OAuth/API key form, progressive disclosure
- Plan 3: Encrypted credential storage (AES-256-GCM) + Anthropic API validation

**Plans**: 12-01 through 12-03

### Phase 13: Error Resolution Engine (ERE)
**Goal**: Systemd-like error logging, AI-powered analysis, and automated correction
**Research**: Unlikely (builds on existing logging + Access Portal credentials)
**Scope**: Large — 3 plans

**Sub-goals**:
- Plan 1: Structured error logging from all pages with categorized storage
- Plan 2: Error analysis via Claude API + overlay popup for user confirmation
- Plan 3: In-session fix execution + out-of-session startup fix injection

**Plans**: 13-01 through 13-03
