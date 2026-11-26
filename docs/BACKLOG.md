# Backlog

Ideas and polish items that are not yet ordered in `ROADMAP.md`. Each entry
should be small enough to pick up without re-scoping the roadmap.

## UI — Diff viewer

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
as a small badge overlay on the message field (`↑… ↓…`). There is no user
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

## UI — Status bar

### Space between operation message and cancel control

**Problem:** While a background operation is running, the status text (e.g.
«Pull in progress…») sits flush against the cancel disc on the right side of the
status bar. There is no visual gap between the message and the control.

**Goal:** Add comfortable horizontal spacing between the truncated operation
label and the cancel button so the two controls read as separate affordances.

**Likely touch points:** `src/ui/status_bar.rs` — layout around
`show_operation_cancel` and `item_spacing`; layout test
`operation_cancel_follows_the_message_in_the_status_bar`.

**Status:** Not started

### Hide AI provider icon when signed out

**Problem:** After signing out of a subscription provider (e.g. GitHub
Copilot), the status bar can still show the provider icon (GitHub / OpenAI) to
the left of the repository path. The indicator follows saved “subscription”
settings in `connection_indicator`, not whether a session is still valid
(screenshot from 2026-11-26).

**Goal:** Show the clickable provider icon only while the user is signed in for
that provider. After logout (or expired session with no usable token), remove the
icon and separator so the status bar matches the disconnected state. Clicking
Settings should still offer sign-in; do not imply an active session when there is
none.

**Constraints:** Preserve the rule that painting the status bar must not
trigger heavy credential reads on every frame; use existing connection/session
state updated at login, logout, and generation time.

**Likely touch points:** `src/ui/status_bar.rs` (`connection_indicator`,
`show`), `src/app` AI connection status, settings sign-out flows,
`indicator_follows_the_saved_provider_for_the_open_repository` and related tests.

**Status:** Not started

## Dependencies

### Drop the egui and winit git patches

**Problem:** `Cargo.toml` replaces crates.io builds with two unmerged upstream
fixes. Neither is in a release `eframe` 0.37 can use.

- `eframe` and `egui_extras` come from `quazardous/egui` at `5a5cc7f`
  ([emilk/egui#8890](https://github.com/emilk/egui/pull/8890)). Official
  `eframe` only exposes the root window (`Frame::winit_window`, since 0.34.3).
  Child viewports need `eframe::viewport_winit_window`, which that PR adds.
  Latest release is 0.37.0 (2026-10-15); the PR is still open.
- `winit` comes from `rust-windowing/winit` at `39c4009`
  ([rust-windowing/winit#4694](https://github.com/rust-windowing/winit/pull/4694)),
  a 0.30 backport of the Windows mixed-DPI drag fix. Stable 0.30.13 does not
  include it. The same class of fix is on master
  ([#4341](https://github.com/rust-windowing/winit/pull/4341)) and in
  `0.31.0-beta`, which `eframe` 0.37 does not support.

**Goal:** When each fix ships in an official release that this app can depend
on, delete that `[patch.crates-io]` entry and use the crates.io crate. Drop the
egui fork only once `viewport_winit_window` (or an equivalent child-viewport
handle) is in the `eframe` version we take. Drop the winit pin only once the
mixed-DPI drag fix is in the `winit` 0.30 line `eframe` requires, or once
`eframe` itself moves to a `winit` that already has it. The two can land
separately.

**Likely touch points:** `Cargo.toml` `[patch.crates-io]`, `Cargo.lock`,
call sites of `eframe::viewport_winit_window` (`src/native_window_linux.rs`,
`src/ui/branches.rs`).

**Status:** Not started