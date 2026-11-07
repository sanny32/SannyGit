# SannyGit

SannyGit is a simple, fast, modern, open-source Git GUI client built with Rust and egui.

## Current status

Stages 1–5 connect the Working Tree interface to the installed Git executable:

- Use **Repository → Add or Create…** (or **+** in Repositories) and enter a local working-tree directory. A subdirectory, an unborn repository, and a linked worktree are supported; bare repositories are not.
- The menu order follows SmartGit: Repository, Edit, View, Remote, Local, Branch, Query, Tools, Review, Window, Help. Available actions share the same guards as their toolbar/panel controls; future actions are disabled.
- On macOS, these are native menus in the system menu bar (Add or Create: ⌘O; Refresh: F5). Windows and Linux show these menus above the toolbar.
- **New Window** (⌘N on macOS) starts an independent empty application window. Secondary windows use isolated temporary session storage, so they do not overwrite the primary window's saved repositories/groups.
- **Add or Create…** (⌘O) opens an existing working tree or initializes a new or empty directory. Existing repositories are not reinitialized. **Clone…** (⇧⌘O) follows the SmartGit three-step wizard: Repository, Selection, Local Directory. It inspects remote branches in the background, supports choosing a branch, submodules and all-heads/tags fetching, and provides a native folder chooser. The destination must not yet exist. Hosting-account shortcuts are disabled; use URL or Path. The clone opens when finished. Credential helpers/SSH remain in use; URL-embedded credentials are refused.
- **Add Group…** creates a persistent repository group. Select the destination group when adding repositories, or right-click a repository to move it to a group/Ungrouped. **Search for Repositories…** discovers working trees and linked worktrees below a chosen directory; select results before adding them.
- **Close Repository** clears the working view and keeps its saved entry/groups. An unsaved commit-message draft requires confirmation before discarding it. **Close** closes the application window.
- **Add/Remove Worktree…** opens a worktree manager with explicit creation and removal actions. New worktrees can use an existing local branch or create one from HEAD. Removal requires confirmation, checks for local/untracked/ignored files, and refuses the main, active, bare or locked worktree. Branches are kept. **Prune Obsolete Worktrees…** shows a dry-run preview before pruning missing, unlocked worktree metadata with `--expire=now`; it does not delete working directories.
- **Settings…** edits repository-local `user.name`/`user.email`; blank values remove the local override. **Edit Git Config → Repository/User…** opens the common repository config or user config in the system text editor (macOS: text editor; Windows: Notepad; Linux: default file handler). A missing user config is created empty after this explicit action; other config options can be edited there.
- Select a saved repository to open it again. Repository paths and the selected theme are persisted locally.
- View the current branch (including detached HEAD), changed files and staged status. **View → Refresh** reloads changes made outside the application.
- Select a file to view separate staged and working-tree unified patches. Untracked text files have a preview; binary files show a notice. Conflict previews compare the working tree with ours.
- Use **Stage / Unstage** for the selected file, or toggle its **Index** checkbox. Unstage preserves working-tree contents, including before the first commit. A dash indicates both staged and working-tree changes: clicking it unstages the file; Stage includes its remaining changes.
- Enter a message and click **Commit** to commit all staged changes. The header shows the staged-file count. Unstaged changes are left out. Empty messages and unresolved conflicts block committing.
- Git operations run on background threads. The index and diff refresh after each write attempt. A failed commit retains its message; a successful commit clears it. Git identity, hooks and signing configuration remain in effect.
- **Journal** loads the newest 500 commits in topological order from the current branch/HEAD or all branches. Its graph follows actual parent links. Search matches the loaded messages, author names/emails and hashes; connections are hidden while filtering.
- Click a Journal row to open **Commit details** with the full message, author, date, hash, parents and a read-only diff. Merge commits are compared with their first parent. Root commits (and shallow boundaries) show the available root patch. Copy hash copies the full object ID.
- History reloads after repository refresh and index/commit operations; **Reload** refreshes Journal alone. A failed history read retains previous rows with an error and does not block repository writes. Large commit diffs report the display limit and can be retried separately.
- **Branches** loads local branches, remote-tracking branches and tags, marks the checked-out local branch and shows upstream/object information in tooltips. Symbolic aliases such as `origin/HEAD` are hidden. Filter matches names and upstreams; the reload icon refreshes repository state and refs together.
- Select a local branch and click **Switch**. This stage requires a clean index and working tree, including untracked files. Git refuses overwriting ignored files and branches occupied by other worktrees; force, auto-stash, remote guessing and submodule recursion are not used.
- **+** opens **Create branch**, starting from current HEAD. Choose whether to switch to the new branch. Creation without switching also works with local changes; creation with switching requires a clean tree. Create the first commit before creating additional branches. Duplicate/invalid names are reported without replacing existing branches.
- Remote branches and tags are currently read-only. Branch creation/switch attempts refresh status, refs and history; unsuccessful attempts retain the branch-name draft and show an error.
- Errors appear above the work area. If refresh fails, the previous view remains visible and write actions are disabled until a successful refresh.

The application starts with an empty repository list and uses real Git data after opening a repository. Mock data is used only by tests. Branch deletion/renaming, remote tracking setup, tag operations, amend, conflict resolution, partial-hunk staging and remote synchronization operations are not implemented yet; their actions remain disabled or hidden. Repository writes happen only after an explicit menu/panel action.

Display limits: 5,000 refs; 500 loaded commits; 5,000 changed files; 2 MiB per Git command or file preview; 10,000 diff lines; 16 KiB per diff line. Ordinary Git commands time out after 15 seconds; clone allows 10 minutes and worktree creation 2 minutes. Clone, search and worktree creation can be cancelled; partial destinations may remain after failure/cancellation and are never automatically deleted. Searches skip symlink directories and `.git`, `target`, `node_modules`; they are bounded by depth (default 6, maximum 16), 20,000 directories, 100,000 entries, 200 repositories and 30 seconds, with any active Git read also subject to its command timeout. Non-UTF-8 paths and text are reported as unsupported. There is no automatic filesystem watcher yet: use Refresh to update the view.

## Requirements

- Rust 1.95 or newer
- Git 2.36 or newer installed and available on `PATH` (for NUL-delimited [worktree porcelain output](https://git-scm.com/docs/git-worktree/2.36.0))
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
