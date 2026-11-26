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

## UI — Changes list

### Grouped file path draws too far to the right

**Problem:** With directory grouping on, a file row under its folder header
paints the path far to the right of the File column. The file icon stays on the
left, then a wide gap, then truncated text such as `assets/icons/fo` while the
header above reads `assets/icons` in the normal place (screenshot from
2026-11-26, Files list, `assets/icons` group). The path is clipped instead of
starting next to the icon.

**Goal:** Keep the file icon and path left-aligned in the File column, on the
same inset as other file rows. Truncate only when the column is actually too
narrow, and show the full path in the tooltip.

**Likely touch points:** `src/ui/changes.rs` — file row in the changes table
(`row.col` for the path) and `paint_directory_header` (row-layer text that is
not clipped to the File cell).

**Status:** Not started