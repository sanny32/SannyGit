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

### Local branch rows: folder groups and inline tracking (SmartGit-style)

**Problem:** Local branches are listed flat under **Local Branches** with only the
short ref name. Upstream/tracking data (`BranchRef::upstream`, ahead/behind) is
not shown on the row — only in tooltips or the status bar for the current
branch. SmartGit groups slash-prefixed locals (e.g. **Features (1)**) and paints
rows such as `server-node-icons ? origin`: branch name in body text, the remote
name (**origin**) in accent/muted color, and a **red ?** only when the
configured upstream ref on that remote no longer exists (deleted remote branch)
(screenshot from 2026-11-26).

**Goal:** Match that presentation for local branches: optional nested groups by
path segment (`feature/foo` → group **Features**), count in the header, remote
name on the row when an upstream is configured, and **only** the red **?**
suffix for the “upstream gone on origin” case — not for unknown ahead/behind,
not for in-sync branches, and not a general status badge on every local branch.
Truncate with tooltip on elision; keep selection, double-click checkout, and
context menus working.

**Likely touch points:** `src/ui/branches.rs` (`branch_group`, branch row
layout), `src/branches.rs` / `src/models.rs` (upstream fields), detection of
missing remote upstream (fetch/ls-remote or existing ref loads), layout tests
under `src/ui/branches.rs`.

**Status:** Not started

### Collapsing group rows: gap between chevron and title

**Problem:** Section headers that use the shared disclosure chevron (**Local Branches**,
**Remote Branches**, **Tags**, and the same pattern elsewhere) place the title
text too close to the chevron on the right. At a narrow panel width the labels
even read as clipped prefixes (**Lo**, **Re**, **Ta**) with almost no air between
glyph and copy (screenshot from 2026-11-26, Branches panel).

**Goal:** Add a modest horizontal gap between the chevron and the following title
text on collapsing headers so labels read clearly and match SmartGit spacing.
Apply consistently wherever `icons::collapsing_icon` is used; avoid changing tree
indent or row height unless needed for alignment.

**Likely touch points:** `CollapsingHeader` layout and custom
`icons::collapsing_icon` (`src/ui/controls/icons.rs`), call sites such as
`src/ui/branches.rs` (`branch_group`), `src/ui/repositories.rs`,
`src/ui/changes.rs` (directory header chevron + path), `src/ui/settings.rs`,
`src/ui/merge.rs`, `src/ui/remote_dialog.rs`; egui header internals if the gap
must be fixed in the fork.

**Status:** Not started

### Deleting a branch on origin fails with an error

**Problem:** Removing a branch on **origin** through the Branches UI (for example
**Delete…** on a local branch with **Delete from remote repository 'origin'**, or
the equivalent reviewed delete flow) ends in an **Attention required** modal
instead of completing the push delete and refresh.

**Goal:** Remote deletion on the configured remote should succeed when Git and
permissions allow it (`git push origin :branch` / force-with-lease path in
`remotes::delete_branch`). If the user only deletes a **remote-tracking** ref
under **Remote Branches**, behavior must match SmartGit expectations (server
delete vs local ref only) and must not surface spurious errors. Reproduce,
capture the exact Git diagnostic, fix root cause, and add regression coverage.

**Likely touch points:** `src/remotes.rs` (`delete_branch`, `preview_branch`,
push `--force-with-lease`), `src/app/ref_management.rs` (`BranchAction::Delete`,
`run_ref_action`), `src/ui/ref_management.rs` / `src/ui/branches.rs`
(`DeleteRemoteBranch`, confirm dialog), `src/remotes/tests.rs`.

**Status:** Not started

## UI — Settings (dark theme)

### Keyboard Shortcuts table matches text field background

**Problem:** In dark theme, **Settings → Keyboard Shortcuts** paints the command
table inside a frame filled with `palette.panel` (lighter gray), while the
**Filter** field and **Shortcut** assign field use the standard text-edit
background (darker, same as other single-line inputs). The mismatch is obvious in
the scrollable list area (screenshot from 2026-11-26).

**Goal:** In dark theme (and keep light theme consistent), use the same fill as
`TextEdit` / `singleline` for the shortcuts table frame and row area — or
otherwise align table and edit backgrounds. Header row and borders can stay as
now if needed for column separation.

**Likely touch points:** `src/ui/settings.rs` (`keyboard_shortcuts_page`,
`Frame::fill(palette.panel)` around the list), `src/ui/settings/tests.rs`
(`shortcut_page_uses_the_same_gray_as_other_pages`).

**Status:** Not started

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