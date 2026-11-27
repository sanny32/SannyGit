# SannyGit Agent Instructions

These instructions apply to the entire repository.

## Project

Compact cross-platform Git desktop client inspired by SmartGit Working Tree.
Windows/Linux/macOS; English UI; MIT. Rust 2021, MSRV 1.95; eframe and
egui_extras 0.37; import egui through `eframe::egui`.
Preserve Git CLI operations and configured OpenAI-compatible/local message
generation with system credential storage. Commit/Amend default to all changes;
Staged only preserves unstaged changes and controls message generation.
README covers setup and major capabilities; BACKLOG lists open polish/UX work.
ROADMAP does not authorize additional implementation. Ignored local requirements
`SannyGit-Project.txt` and `reference-1.png` / `reference-2.png` may be absent.

## Read on demand

Read these mandatory rules before the indicated work, even outside `src/ui`:

| Work | Instructions |
| --- | --- |
| UI, dialogs, themes, menus, shortcuts, UI-facing app state/errors | [UI rules](.agents/docs/ui.md) |
| Git execution/parsing, repository operations, background workers | [Backend rules](.agents/docs/backend.md) |
| Settings storage, persisted paths/formats, migrations | [Persistence rules](.agents/docs/persistence.md) |
| Finding modules (optional lookup) | [Source map](.agents/docs/source-map.md) |

Load only task-relevant skills and references, not all documents upfront.
Project dependency versions take precedence over older skill examples.

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
- For any test or manual verification that touches a real Git working tree, agents
  must use their own dedicated test repositories — isolated temporary directories
  created for the run, or fixtures under `tests/` — never this project's working
  copy (`D:/Work/SannyGit` or the clone path in use) and never the user's personal
  or production repositories. Automated tests already follow this; agents doing
  exploratory `cargo run`, scripted scenarios, or one-off Git commands must do
  the same.
- Do not create test commits, tags, branches, remotes, or other mutating Git
  operations in the project workspace or in repositories the user keeps outside
  the task scope.

## Development Rules

- Keep all local review notes, drafts and findings under `.review/`.
- Search narrowly with `rg` and read relevant ranges. Expand scope as needed;
  avoid full-file/log dumps and reuse findings already obtained in this task.

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
- When a task implements or fully resolves an item in `docs/BACKLOG.md`, remove
  that entry from the backlog in the same change set. Do not leave completed
  items marked “Not started” or add a done history inside the backlog file; the
  backlog lists only open work. Add new backlog items there when the user asks
  or when follow-up polish is explicitly deferred.
- Do not change `.gitignore` rules or the `Cargo.lock` tracking policy without
  a task-related reason. Do not add `target/` or other build artifacts.

## Verification and Task Completion

After changing Rust code, run these checks against the final code:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Use focused checks during iteration; they do not replace the final set above. Reuse successful checks while their inputs are unchanged; rerun affected checks after edits. Keep successful output brief and inspect failure details as needed. For documentation-only changes, check content, links and diff; Rust checks are optional.

Use `cargo fmt` to fix formatting. For manual UI verification, run `cargo run`
when a graphical environment is available.

In the Codex sandbox used for this workspace, the full test suite needs local
TCP listeners for its HTTP-server tests. Sandboxed runs have repeatedly failed
with `PermissionDenied` / `Operation not permitted` when binding these listeners.
Run `cargo test` with `sandbox_permissions: "require_escalated"` from the outset
when this restriction applies, using the normal approval mechanism. Do not
repeat a known-failing sandboxed run just to rediscover this limitation. Treat
other test failures separately; do not assume they are sandbox restrictions.

Add meaningful tests when changing diff parsing, Git output parsing, status
handling, branches, or error handling. Keep unit tests next to the code and
place integration tests in `tests/` when needed. Tests and agents must not depend
on or modify the agent's checkout of SannyGit itself; provision separate
repositories under `tempfile`, `tests/fixtures`, or another disposable path.

In the final response, briefly describe the result, list created and modified
files, and report the checks performed and their results. Mention checks that
could not be performed and any remaining limitations. Do not claim to have
launched the GUI or passed tests without actually verifying them.
