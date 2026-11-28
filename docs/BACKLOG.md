# Backlog

Ideas and polish items that are not yet ordered in `ROADMAP.md`. Each entry
should be small enough to pick up without re-scoping the roadmap.

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

## UI — Branches panel

### Local branches: upstream comparison sign and commit delta

**Problem:** Local branch rows show the branch name and, when set, the remote
label inline (`dev origin`), but not how the branch relates to that upstream.
SmartGit paints an operator between name and remote — **`=`** when even, **`<`**
when behind, **`>`** when ahead — and a compact count pill on the right (e.g.
**main < origin** with **-46** when 46 commits behind) (screenshot from
2026-11-27, **Local Branches**). Today only the current branch gets ahead/behind
via the status bar (**↑n ↓m**); other locals with tracking show no row-level
delta.

**Goal:** For each local branch with a configured upstream, show the SmartGit-style
comparison sign after the branch name (before or with the remote short name) and
the numeric difference on the trailing edge of the row (**-n** behind, **+n**
ahead, omitted or **0** when in sync). Keep the red **?** for missing upstream on
the remote; use muted/accent colors consistent with the status bar. Unknown counts
may show **?** until refreshed. Truncate with tooltip including full upstream and
counts.

**Likely touch points:** `src/ui/branches.rs` (`branch_row_contents` / row layout),
`src/models.rs` (`BranchRef` — optional ahead/behind per ref), batch or cached reads
building on `src/tracking.rs` (`read`, `git rev-list --left-right --count`),
`src/branches.rs` (ref load after fetch), layout tests under `src/ui/branches.rs`.

**Status:** Not started

## UI — Branches panel (repository switch)

### Show full ref list immediately when changing repository

**Problem:** After selecting another repository, the Branches panel often shows
only **Tags** (empty or stale) for a moment while locals and remotes are absent.
`branches.entries` is cleared on switch and refs load asynchronously via
`load_refs`, and `branch_group` hides empty **Local Branches** / **Remote Branches**
sections but still paints **Tags**. The result looks like lazy, tag-only loading
instead of the full tree.

**Goal:** On repository change, present local branches, remote branches, and tags
together as soon as the panel is visible — either keep showing the last loaded
snapshot for that repo until the refresh completes, show all three section headers
with a stable loading state, or block painting until `branches::load` returns.
Avoid an intermediate UI that lists tags alone.

**Likely touch points:** `src/app.rs` (`apply_snapshot`, `branches.entries.clear`,
`reload_refs` / `load_refs`), `src/app/repositories.rs` (`reset_repository_view`),
`src/ui/branches.rs` (`branch_group`, `local_folder_groups`, Tags special-case),
`BranchState::apply`, tests in `src/app/working_tree.rs` and
`src/app/repositories.rs` around repo switch.

**Status:** Not started
