# Source Map
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
| `src/preferences.rs`, `src/persistence.rs`, `src/llm.rs` | User settings (JSON on disk), commit-message generation and credential access |
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

