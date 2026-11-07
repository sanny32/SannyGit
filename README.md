# SannyGit

SannyGit is a simple, fast, modern, open-source Git GUI client built with Rust and egui.

## Current status

Stages 1–2 connect the Working Tree interface to the installed Git executable:

- Use **Open** (or **+** in Repositories) and enter a local working-tree directory. A subdirectory, an unborn repository, and a linked worktree are supported; bare repositories are not.
- Select a saved repository to open it again. Repository paths and the selected theme are persisted locally.
- View the current branch (including detached HEAD), changed files and staged status. **Refresh** reloads changes made outside the application.
- Select a file to view separate staged and working-tree unified patches. Untracked text files have a preview; binary files show a notice. Conflict previews compare the working tree with ours.
- Use **Stage / Unstage** for the selected file, or toggle its **Index** checkbox. Unstage preserves working-tree contents, including before the first commit. A dash indicates both staged and working-tree changes: clicking it unstages the file; Stage includes its remaining changes.
- Enter a message and click **Commit** to commit all staged changes. The header shows the staged-file count. Unstaged changes are left out. Empty messages and unresolved conflicts block committing.
- Git operations run on background threads. The index and diff refresh after each write attempt. A failed commit retains its message; a successful commit clears it. Git identity, hooks and signing configuration remain in effect.
- Errors appear above the work area. If refresh fails, the previous view remains visible and write actions are disabled until a successful refresh.

The application starts with an empty repository list and uses real Git data after opening a repository. Mock data is used only by tests. Branch management, history, amend, conflict resolution, partial-hunk staging and remote operations are not implemented yet; their actions remain disabled or hidden. Repository writes happen only after an explicit Stage, Unstage or Commit action.

Display limits: 5,000 changed files; 2 MiB per Git command or file preview; 10,000 diff lines; 16 KiB per diff line. Git commands time out after 15 seconds. Non-UTF-8 paths and text are reported as unsupported. There is no automatic filesystem watcher yet: use Refresh to update the view.

## Requirements

- Rust 1.95 or newer
- Git 2.23 or newer installed and available on `PATH` (for `restore --staged`)
- A supported desktop platform: Windows, Linux, or macOS

The UI uses eframe/egui 0.37 with font hinting enabled for sharper small text.

## Build and run

```sh
cargo build
cargo run
```

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

SannyGit is distributed under the MIT License.
