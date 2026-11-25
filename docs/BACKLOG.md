# Backlog

Ideas and polish items that are not yet ordered in `ROADMAP.md`. Each entry
should be small enough to pick up without re-scoping the roadmap.

## UI — Modern theme

### Denser repository list (Modern)

**Problem:** In Modern interface style the Repositories panel uses tall rows
(name and local path on two lines, 44 px row height, generous spacing). With
several open repositories the list feels loose compared to Classic (24 px,
single line).

**Goal:** Tighten vertical rhythm in Modern only — less padding between rows,
and/or slightly smaller row height, without clipping names, paths, sync badges,
or the merge/conflict affordances.

**Reference:** Screenshot from 2026-11-25 (RustRover / local build, light Modern
theme, six repositories in one group).

**Likely touch points:** `src/ui/repositories.rs` (`repository_row`, row
`height` when `palette.panel_radius > 0`), layout tests around
`name_stays_next_to_icon_with_tracking_counts`.

**Status:** Not started

## UI — Diff viewer

### Line selection must not shift layout

**Problem:** Selecting a diff line adds a 1 px accent stroke around the row
frame. The extra border grows the row box, so following lines jump down by a
few pixels. The effect is visible in Unified view when toggling selection on
added/removed/context lines (screenshot from 2026-11-25, `remotes.rs` diff).

**Goal:** Keep every line at a fixed vertical position whether it is selected or
not — draw the highlight inside the same footprint (inset stroke, overlay, or
always-reserved border width).

**Likely touch point:** `src/ui/diff_view.rs`, `show_line` (`Frame::stroke` when
`selected`).

**Status:** Not started

### Show file path on the diff tab, not in the toolbar

**Problem:** The diff toolbar repeats the repository-relative path
(`src/remotes.rs`) on the left, next to `+added`/`-removed` counts and view
controls. The header tab above shows only the file name (`remotes.rs`); the full
path appears only in a tooltip. That duplicates information and crowds the
toolbar row (screenshot from 2026-11-25).

**Goal:** Remove the path label from the diff action toolbar. Show the
repository-relative path in the active file tab (truncate with tooltip on
elision). Keep `+`/`-` stats and Unified / Side by Side / whitespace / Stage /
Unstage / Discard on the second row.

**Likely touch points:** `src/ui/diff_view.rs` — `file_header` (tab label) and
the path `Label` in the toolbar layout around the line actions.

**Status:** Not started

## Commit — AI message generation

### Preference to show token usage in the commit panel

**Problem:** After “Generate commit message”, input/output token counts appear
as a small badge overlay on the message field (`↓… ↑…`). There is no user
setting to turn this on or off. `CommitState` also accumulates
`session_generation_tokens`, but nothing in the commit UI exposes session
totals.

**Goal:** Add a persisted preference (application or commit/AI settings) that
controls whether token statistics are shown in the commit window — at minimum
the last-generation input/output badge; optionally whether to include a session
running total. When disabled, hide the badge and keep layout unchanged.

**Likely touch points:** `src/preferences.rs` (new field + serde), `src/ui/settings.rs`
(AI / commit message section), `src/ui/commit_panel.rs` (`draw_token_usage_badge`,
`last_generation_tokens`), `src/app/commit.rs` (`session_generation_tokens`).

**Status:** Not started