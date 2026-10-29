# SannyGit Agent Instructions

These instructions apply to the entire repository.

## Project and Current Stage

SannyGit is a compact, cross-platform desktop Git client built with Rust and
egui, inspired by the SmartGit Working Tree interface. Target platforms are
Windows, Linux, and macOS. The UI language is English. The license is MIT.

- Rust edition 2021; minimum supported Rust version: 1.95.
- GUI: `eframe` 0.37 and `egui` through `eframe::egui`.
- `egui_extras` 0.37 provides tables and SVG support; `serde` handles serialization.
- The current implementation is a UI foundation with mock data and persisted
  theme selection. Real repository integration and Git command execution are
  not implemented yet.
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
| `src/app.rs` | `SannyGitApp`, state, initialization, UI loop, theme persistence |
| `src/models.rs` | Models for repositories, changes, commits, and diff lines |
| `src/mock_data.rs` | Mock data kept separate from the UI |
| `src/diff.rs` | Unified diff parser and its unit tests |
| `src/theme.rs` | Light and dark themes, palettes, typography, theme storage key |
| `src/ui/mod.rs` | Panel layout and sizing |
| `src/ui/*.rs` | Individual panels: toolbar, repositories, branches, changes, diff, commit, journal, status bar |
| `src/ui/icons.rs` | Loading and rendering embedded SVG icons |
| `assets/icons/` | Existing SVG icon set |

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
- Keep mock data in `src/mock_data.rs`. Do not present it as real Git results;
  update the status description in README when connecting a real backend.
- Do not change `.gitignore` rules or the `Cargo.lock` tracking policy without
  a task-related reason. Do not add `target/` or other build artifacts.

## User Interface

- Keep the OS-provided title bar and window controls. Do not replace them with
  custom controls in the application toolbar.
- Preserve the traditional Working Tree layout: toolbar at the top, repositories
  and branches on the left, changes and diff in the center, commit and journal
  panels below, and a status bar along the bottom edge.
- Favor compact controls, high information density, resizable panels, and clear
  selection states. Avoid oversized cards and unnecessary decoration.
- Support both themes. Define colors through the palette in `src/theme.rs`
  rather than arbitrary constants inside components.
- Use the existing SVG icons and `src/ui/icons.rs`.
- Disable unimplemented actions with a clear tooltip, or hide them. Do not create
  controls that appear functional but do nothing.
- Commits and other repository operations must be initiated by an explicit
  action from the application user. Never commit automatically.
- Check the layout in both themes at the normal window size and the minimum
  size of 1100 x 700. Do not display the compiler version in the status bar.

## Future Git Integration

These rules apply when a task requires backend implementation:

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

Add meaningful tests when changing diff parsing, Git output parsing, status
handling, branches, or error handling. Keep unit tests next to the code and
place integration tests in `tests/` when needed.

In the final response, briefly describe the result, list created and modified
files, and report the checks performed and their results. Mention checks that
could not be performed and any remaining limitations. Do not claim to have
launched the GUI or passed tests without actually verifying them.
