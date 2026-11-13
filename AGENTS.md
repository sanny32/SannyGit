# SannyGit Agent Instructions

These instructions apply to the entire repository.

## Project and Current Stage

SannyGit is a compact, cross-platform desktop Git client built with Rust and
egui, inspired by the SmartGit Working Tree interface. Target platforms are
Windows, Linux, and macOS. The UI language is English. The license is MIT.

- Rust edition 2021; minimum supported Rust version: 1.95.
- GUI: `eframe` 0.37 and `egui` through `eframe::egui`.
- `egui_extras` 0.37 provides tables and SVG support; `serde` handles serialization.
- The application has a Git CLI backend: repositories, clone, status/diff,
  staging, single-file discard, commit/amend, stash, history, branches/tags,
  worktrees, and remotes.
  Mock data is compiled only for tests. Commit and Amend default to all changes;
  an explicit Staged only mode preserves unstaged changes. The selected mode
  also controls commit-message generation.
- Commit-message generation uses an explicitly configured OpenAI-compatible
  service or local server. Preserve this existing feature; credentials use
  system credential storage.
- `README.md` documents running the application and verification commands.
  `SannyGit-Project.txt`, when available locally, contains detailed requirements.
  It and `reference-1.png` / `reference-2.png` are excluded from Git; do not assume
  they are available in another clone.
- Do not treat the development roadmap as an instruction to implement every phase.

## Git Safety

- Leave changes uncommitted. Do not run `git add`, `commit`, `commit --amend`,
  `push`, `merge`, `rebase`, `reset`, `cherry-pick`, `revert`, `tag`, `stash`,
  `clean`, create branches, or switch the working tree without explicit user
  authorization for the corresponding operation.
- Do not modify Git configuration or hooks without permission. Do not
  automatically add a permanent `safe.directory` exception. If an ownership
  error occurs for this trusted workspace, use a command-scoped option:
  `git -c safe.directory=D:/Work/SannyGit status --short`. Substitute the actual
  absolute path in another environment.
- Use read-only commands to inspect the project: `git status`, `git diff`,
  `git log`, `git show`, `git branch --list`, and `git remote -v`.
- Do not overwrite or revert other people's changes. Edit source files only
  within the task scope; do not delete unrelated files or user data.
- Git integration tests must use separate temporary repositories. Do not create
  test commits or perform other mutating Git operations in the project's working
  repository or the user's repositories.

## Source Map

| Path | Purpose |
| --- | --- |
| `src/main.rs` | Entry point, native window options, module declarations |
| `src/app.rs`, `src/app/*.rs` | Application state, background workers, repository/network actions, UI loop |
| `src/models.rs` | Models for repositories, changes, commits, and diff lines |
| `src/mock_data.rs` | Test-only mock data kept separate from the UI |
| `src/git.rs`, `src/git/diagnostics.rs` | Git execution, safe command diagnostics, status, diff, staging and commit/amend |
| `src/repository.rs`, `src/clone_source.rs` | Repository discovery, creation, clone, worktrees, external opening |
| `src/discard.rs`, `src/app/discard.rs`, `src/ui/discard.rs` | Reviewed single-file restore/deletion, worker actions and confirmation dialog |
| `src/stash.rs`, `src/app/stash.rs`, `src/ui/stash.rs` | Stash backend, background actions, previews and management dialog |
| `src/branches.rs` | Branch/tag listing and local operations |
| `src/history.rs`, `src/app/history.rs` | Snapshot-based history pages, graph preparation and background read coordination |
| `src/remotes.rs`, `src/tracking.rs` | Fetch/pull/push, selected-tag push and upstream tracking |
| `src/remote_config.rs`, `src/app/ref_management.rs`, `src/ui/ref_management.rs` | Local remote configuration, reviewed branch deletion and tag-push dialogs |
| `src/preferences.rs`, `src/llm.rs` | Persisted preferences, commit-message generation and credential access |
| `src/diff.rs` | Unified diff parser and its unit tests |
| `src/theme.rs` | Light and dark themes, palettes, typography, theme storage key |
| `src/ui/mod.rs` | Panel layout and sizing |
| `src/ui/*.rs` | Individual panels: toolbar, repositories, branches, changes, diff, commit, journal, status bar |
| `src/ui/controls/` | Shared controls, dialog styles, tooltips, and embedded SVG icons |
| `src/menu.rs`, `src/native_menu.rs` | Application menu actions and macOS native menus |
| `assets/icons/` | Existing SVG icon set |
| `build.sh`, `build.ps1`, `build.rs`, `.github/linux/`, `.github/windows/`, `.github/macos/` | Local build/install scripts and native packaging resources |
| `.github/workflows/build-test.yml`, `.github/workflows/build-*.yaml` | Cross-platform checks and CI distribution packages on stable Rust |

Create new modules only when needed. Do not create empty architectural scaffolding
or mix Git logic with panel rendering.

## Development Rules

- Before making changes, read the affected modules and inspect the working tree.
  For substantial tasks, briefly describe the plan, then implement the smallest
  practical solution.
- Follow the existing style and idiomatic Rust. Avoid unnecessary abstractions,
  duplication, and refactoring unrelated to the task.
- Maintain compatibility with Rust 1.95. Do not upgrade dependencies or add
  libraries unless the current task requires them.
- Handle errors explicitly. Do not use `unwrap()` or `expect()` for recoverable
  runtime errors.
- Keep test mock data in `src/mock_data.rs`. Do not present it as real Git results.
- Keep `README.md` concise. Add only important project information, such as
  setup, build and run instructions, major capabilities, and significant
  limitations. Do not document minor behavior changes, routine fixes, or
  implementation details there; README is not a changelog.
- Do not change `.gitignore` rules or the `Cargo.lock` tracking policy without
  a task-related reason. Do not add `target/` or other build artifacts.
- Perform all local reviews in the `.review` folder. Keep all review-related
  files (notes, drafts, findings) there; do not scatter them across the
  repository.

## User Interface

- Keep the OS-provided title bar and window controls. Do not replace them with
  custom controls in the application toolbar.
- All dialogs except Settings must use the shared header from
  `src/ui/controls/dialog_style.rs` (`header` / `header_with_icon`): the standard
  title, description, icon, typography, background and spacing. Keep this header
  consistent when making dialogs more compact; do not replace it with a custom
  heading. Settings retains its own layout.
- Preserve the traditional Working Tree layout: toolbar at the top, repositories
  and branches on the left, changes and diff in the center, commit and journal
  panels below, and a status bar along the bottom edge.
- Favor compact controls, high information density, resizable panels, and clear
  selection states. Avoid oversized cards and unnecessary decoration.
- Content must not jump during background loading, refreshes, progress updates,
  or errors. Preserve existing rows, scroll position, and control placement;
  show transient status without inserting or removing layout rows. Geometry
  should change only for an explicit user layout action or actual content changes.
- Dialog sizes may change only when collapsed sections are expanded. Loading,
  progress, errors, and action button clicks must not resize dialogs; reserve
  compact, stable space and keep overflowing text accessible (for example, in
  a tooltip). Do not stabilize dialog sizes by adding large empty areas.
- Display all user-facing error messages in modal dialogs, including errors from
  background tasks, settings, previews, and repository operations. Do not show
  error details inline in panels, status bars, tooltips, or operation dialogs.
  Keep the underlying layout stable and preserve drafts and retry controls.
- Support both themes. Define colors through the palette in `src/theme.rs`
  rather than arbitrary constants inside components.
- Use the existing SVG icons and `src/ui/controls/icons.rs`.
- Disable unimplemented actions with a clear tooltip, or hide them. Do not create
  controls that appear functional but do nothing.
- Commits and other repository operations must be initiated by an explicit
  action from the application user. Never commit automatically.
- Check the layout in both themes at the normal window size and the minimum
  size of 1100 x 700. Do not display the compiler version in the status bar.

## Git Backend

These rules apply when extending the existing backend:

- Start with the installed Git executable through `std::process::Command`.
  Pass arguments separately; do not construct shell commands from user input.
- Separate command execution and output parsing from the UI. Do not introduce
  `git2` ahead of an actual need.
- Use machine-readable formats, such as `status --porcelain=v1 -z`,
  `diff --no-ext-diff`, and explicit `log` and `branch` formats.
  Do not parse localized, human-readable output.
- Run Git commands and expensive operations on background threads, passing
  results to the UI through channels. Do not execute commands on every egui frame.
- Provide operation status, clear errors, and limits on loaded diffs and history
  so the interface remains responsive.
- Use existing Git credential helpers and SSH configuration. Never store
  passwords or tokens in plain text or write them to logs.
- Do not add GitHub/GitLab APIs, PR management, AI features, plugins, a terminal,
  a code editor, cloud synchronization, or an advanced rebase UI without a
  separate task.

## Verification and Task Completion

After changing Rust code, run:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Use `cargo fmt` to fix formatting. For manual UI verification, run `cargo run`
when a graphical environment is available. Builds and tests are optional for
changes limited to documentation.

In the Codex sandbox used for this workspace, the full test suite needs local
TCP listeners for its HTTP-server tests. Sandboxed runs have repeatedly failed
with `PermissionDenied` / `Operation not permitted` when binding these listeners.
Run `cargo test` with `sandbox_permissions: "require_escalated"` from the outset
when this restriction applies, using the normal approval mechanism. Do not
repeat a known-failing sandboxed run just to rediscover this limitation. Treat
other test failures separately; do not assume they are sandbox restrictions.

Add meaningful tests when changing diff parsing, Git output parsing, status
handling, branches, or error handling. Keep unit tests next to the code and
place integration tests in `tests/` when needed.

In the final response, briefly describe the result, list created and modified
files, and report the checks performed and their results. Mention checks that
could not be performed and any remaining limitations. Do not claim to have
launched the GUI or passed tests without actually verifying them.
