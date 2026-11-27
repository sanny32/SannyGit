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

## UI — Changes (files list)

### Files header shows literal `{count}` instead of file count

**Problem:** The Changes panel title line reads **Files ({count} files)** with the
placeholder text visible instead of the number of listed files (screenshot from
2026-11-27).

**Goal:** Show the real count, e.g. **Files (3 files)** (or localized equivalent).
Use the same `%{count}` interpolation style as other UI strings (`tr_args` replaces
`%{name}` only). Audit similar keys so counts are not left as `{count}` in the
source or locale files.

**Likely touch points:** `src/ui/changes.rs` (`changes_title` — key
`({count} files)`), `src/i18n.rs` (`tr_args`), locale YAML if the string is
overridden there; same pattern in `src/ui/ignore.rs` (`{count} files selected`).

**Status:** Not started

## UI — Settings (dark theme)

### Menu shortcut labels: spacing between modifier glyphs and keys

**Problem:** Shortcut text on menu rows (e.g. **F11**, **⇧⌘PageDown**, **⌘PageDown**) packs
modifier symbols (⇧, ⌘, …) flush against the following key name with no visual
breathing room. **⇧⌘PageDown** reads as one tight cluster instead of separate
parts; the column looks cramped compared to polished native menus (screenshot from
2026-11-26, Edit menu shortcut column).

**Goal:** Render shortcut labels with consistent, modest spacing between modifier
glyphs and key names (and between modifier tokens where appropriate) so the
column looks deliberate and readable. Match SmartGit / platform conventions where
possible; keep alignment in the shortcut column unchanged.

**Likely touch points:** `eframe::egui::Context::format_shortcut` output and
`Button::shortcut_text` layout (`src/ui/toolbar.rs`, `src/ui/controls/popup_menu.rs`,
`src/ui/branches.rs` context menus); if spacing is inside egui’s menu button
painter, the `quazardous/egui` fork or a local wrapper around shortcut display.

**Status:** Not started

### About dialog body uses text-edit black in dark theme

**Problem:** In the **About SannyGit** window, the main content background reads as
the light “card” surface (white in light theme). In dark theme it stays on
`palette.panel` / header grays, so it looks lighter than single-line **TextEdit**
fields elsewhere (screenshot from 2026-11-26).

**Goal:** In dark theme, paint that body background with the same fill as text
inputs (`text_edit_bg_color` or the palette token used by edits). Keep light
theme as today (white/light panel). Footer bar, links, and branding layout stay
unchanged.

**Likely touch points:** `src/ui/about.rs` (`CentralPanel` / `contents` frames,
`body` columns), `src/theme.rs` if a shared token is preferable.

**Status:** Not started

## UI — Settings navigation

### Icons on top-level settings tree nodes

**Problem:** The Settings window left nav is a collapsible tree (`section` /
`leaf` / `entry` in `src/ui/settings.rs`). Top-level rows — **User Interface**,
**Keyboard Shortcuts**, **Git**, and **AI** — show only a disclosure chevron (or
indent) and text. Child pages (Appearance, Remote, …) stay text-only; the top
nodes have no category icons unlike SmartGit-style settings.

**Goal:** Add a consistent SVG icon beside each top-level nav label (before or
after the chevron, aligned with existing row height and `nav_title_offset`).
Reuse or extend `src/ui/controls/icons.rs`; pick icons that read clearly at ~14–16
px in light and dark themes. Nested entries need not get icons unless needed for
alignment.

**Likely touch points:** `src/ui/settings.rs` (`section`, `leaf`, navigation
`ScrollArea` in `show`), `assets/icons/`, layout tests in
`src/ui/settings/tests.rs` (nav column alignment).

**Status:** Not started

### Unify all settings pages to one design system

**Problem:** The Settings window mixes several page layouts (`page`, `heading`,
`section`/`entry`, keyboard-shortcuts table, repository **AI** override pages,
LLM forms). Spacing, headings, muted helper text, control widths, scroll
regions, and background fills differ between **General**, **Appearance**,
**Git**, **Remote**, **AI**, **Commit messages**, **Context**, and **Keyboard
Shortcuts**. Some pages already have visual/layout tests; others do not.

**Goal:** Audit every `SettingsPage` panel in the Settings dialog (application
defaults and per-repository AI when open). Bring them to a single design
system: consistent page title, section grouping, label/control pairs, primary
actions, palette usage, and stable dialog height per [`.agents/docs/ui.md`](../.agents/docs/ui.md)
and the project **egui-ui-design** skill (layout, typography, both themes,
minimum window). Settings keeps its own shell (not `dialog_style` headers), but
inner pages should share one pattern. Document the chosen pattern in code
comments or a short note in `.agents/docs/ui.md` if needed.

**Likely touch points:** `src/ui/settings.rs` (all `SettingsPage::*` arms,
`repository_ai_page`, shared helpers), `src/ui/settings/tests.rs`, `src/theme.rs`,
`src/app.rs` (`SettingsPage`, `SettingsWindow`).

**Status:** Not started

## UI — Repositories panel

### Provider icons share the same visual size

**Problem:** In the repository list, rows use `Icon::for_provider` (e.g. GitHub)
or the generic `Icon::Repository` at a fixed 16 px layout size, but the SVG
glyphs have different intrinsic padding and aspect ratios. GitHub marks look
larger or heavier than the generic diamond/icon; inactive (muted) rows amplify
the mismatch (screenshot from 2026-11-27, mixed GitHub and local repos in one
group).

**Goal:** Normalize perceived icon size across providers: same visual bounding
box, centering, and stroke weight in the row gutter for active and inactive
repositories. Color may still reflect selection; size and alignment should not
shift between GitHub, GitLab, and default repository icons.

**Likely touch points:** `assets/icons/` SVG viewports, `src/ui/controls/icons.rs`
(`image`, provider mapping), `src/ui/repositories.rs` (`repository_row`, 16.0
icon paint and drag preview).

**Status:** Not started

## Performance — upstream badge (↑ ↓)

### Faster ↑ / ↓ counts after commit and push

**Problem:** The ahead/behind badge (**↑n ↓m** in the status bar and on the active
repository row) often stays empty for about **500–1000 ms** after a successful
**Commit** or **Push**, then updates. Users expect the counts to reflect the new
HEAD immediately (screenshot from 2026-11-27: **↑1 ↓0**).

**Goal:** Show updated local ahead/behind within one frame or a single fast
`rev-list` pass after commit and push complete, without waiting on the periodic
tracking worker or optional auto-fetch. Background refresh may still refine
numbers against the remote; avoid clearing the badge to empty during that gap.

**Likely touch points:** `src/app/tracking.rs` (`Monitor::update`, `invalidate`,
`next_read` / `from_secs(5)` throttle), `src/app.rs` (`apply_snapshot` after
commit, push/pull/fetch completion), `src/tracking.rs` (`read`, `badge`),
`finish_ref_operation` / remote job outcomes for **Push complete**.

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