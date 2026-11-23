# SannyGit roadmap compared with SmartGit

Compared on 2026-11-22.

The reference is the installed application `/Applications/SmartGit.app`,
build 261056, dated 2026-09-29. Command names come from the English catalog
`i18n/messages.pot` inside `Contents/Resources/Java/smartgit.jar`. SannyGit
status comes from the current source, chiefly `src/menu.rs` and the panels
under `src/ui/`.

SannyGit follows SmartGit's Working Tree window: toolbar, repositories,
branches, changes, diff, commit, journal, status bar. The Log window in
SmartGit is a separate history window. SannyGit has a Journal plus a history
window opened from a branch or tag. SmartGit's Standard window, DeepGit,
hosting-provider review, and license tooling are recorded here and are not
the next Working Tree work.

This file is a comparison and a proposed order of work. It is not an
instruction to implement every phase.

## Already in SannyGit

| Area | What works |
| --- | --- |
| Repository | Add or create, clone, search, close, groups, move between groups, reveal in the file manager, copy path, open in a new window |
| Worktrees | Add, remove, prune |
| Working tree | Status, unified diff, stage and unstage whole files, ignore, commit and amend, staged-only commit, OpenAI-compatible commit messages |
| Discard | One regular file or symbolic link, after review, up to 2 MiB |
| Stash | Save, including untracked files, apply, pop, drop, with a preview |
| History | Journal graph, search over loaded commits, history window for a selected ref, 500 commits per page, 10,000 loaded commits maximum |
| Branches | Create, switch, rename, delete a branch that is already merged into HEAD |
| Tags | Lightweight tag, checkout as detached HEAD, push, push to, delete |
| Merge | Create merge-commit, merge into the working tree, abort, text conflict solver. Fast-forward is offered when merging a tag |
| Rebase | Replay the current clean branch onto one selected commit. Conflicts are finished outside SannyGit |
| Reset | Reset and Reset Advanced for a remote branch or a tag, including soft, mixed, and hard |
| Remote | Fetch, fast-forward-only pull, push of the reviewed branch tip without `--force`, push of a selected tag, sync as push then fast-forward pull, set upstream, add, edit, and remove repository-local remotes |
| Settings | Light and dark themes, Git executable, date format, confirmation skips, repository and user Git config |

## Gap by SmartGit menu

Status means the SannyGit command, not a partial visual similarity.

### Repository

| SmartGit command | SannyGit |
| --- | --- |
| Add or Create, Clone, Search, Close, Settings, Edit Git Config | Present |
| Add / Remove / Prune Worktree | Present. Branches that belong to another worktree are not marked, and checkout does not open that worktree |
| Mark as Favorite, Rename | Menu rows exist and are disabled |
| Open in Terminal, Open in VS Code | Terminal row is disabled. There is no editor command |
| Select Obsolete Repositories | Absent |
| Group rename, delete, and reorder | Add Group and Move To exist. The repositories header button is disabled |

### Local and Changes

| SmartGit command | SannyGit |
| --- | --- |
| Stage, Unstage, Commit, Ignore | Present for whole files |
| Discard | One file or symlink. Directories, submodules, and conflicts are refused |
| Stash All / Apply / Drop | Present, together with pop |
| Stash Selection, Rename Stash | Absent |
| Index Editor: stage, unstage, or discard a hunk, line, inner-line change, or selection | Absent. The diff header button is disabled. The working-tree diff is unified only |
| Changes view: side by side, compact, whitespace, line endings, recursive directories, separate index and working tree, show unchanged, ignored, or skipped files | Status filters exist: All, Modified, Added, Deleted, Conflicted. The view-options button is disabled. Side by side exists only in the history window |
| Clean Working Tree | Absent |
| Move or Rename | Absent |
| Toggle Assume Unchanged, Toggle Skip Worktree | Absent |
| Undo Last Commit | Amend exists. A one-step undo of the last commit does not |

### Branch, rebase, and history rewriting

| SmartGit command | SannyGit |
| --- | --- |
| Add Branch, Check Out, Rename, Delete | Present. Delete refuses a branch that is not merged. There is no force delete |
| Add Tag | Lightweight tags only. Annotated and signed tags are not created |
| Merge: Create Merge-Commit, Merge to Working Tree | Present |
| Fast-Forward, Squash | Fast-forward is available for a tag. The main merge dialog has no fast-forward or squash action |
| Rebase HEAD to a commit | Present for a clean branch and a single commit |
| Rebase Interactive From, reorder, squash, edit message, edit author, split, modify commit | Absent. The rebase command disables interactive options, autostash, and update-refs |
| Continue, Skip, Abort, Step during rebase, cherry-pick, or revert | Absent. A conflict tells the user to continue or abort in a terminal. Merge abort exists |
| Cherry-Pick, Revert | Absent. An in-progress cherry-pick or revert only blocks other actions |
| Reset, Reset Advanced | Present from a remote branch or a tag. A local branch context menu has Rename and Delete only |
| Set Tracked Branch, Stop Tracking | Upstream can be set from the push dialog |
| Git-Flow: feature, release, hotfix, support | Absent |
| Local branch context menu: Merge, Rebase, Log, Push, Reset, Copy Message, Copy ID | Present |

### Remote

| SmartGit command | SannyGit |
| --- | --- |
| Fetch, Push, Push To, Manage Remotes | Present. Push uses `--no-force` |
| Pull with merge | Fast-forward only. Diverged branches must be merged separately |
| Pull with rebase | Radio button is disabled |
| Fetch All, Fetch More, Push Up To | Absent |
| Synchronize as pull-then-push and push-then-pull | Sync is push, then a fast-forward pull |
| Remote rename as a first-class command, connection properties | URLs can be edited. Renaming the remote is not a separate command |
| Push to Gerrit | Absent |

### Query, journal, and log

| SmartGit command | SannyGit |
| --- | --- |
| Log, graph, current branch and all branches | Present, with the load limits above |
| Journal: incoming, outgoing, and rewritten commits | Ahead/behind tracking exists. The journal does not color incoming, outgoing, or rewritten commits, and it has no second comparison branch |
| Blame | Menu item is disabled |
| Commit context menu: Log, Merge, Rebase, Reset, Reset Advanced, Push, Copy Message, Copy ID | Present on a journal commit and in the history window. Cherry-pick, revert, checkout, new branch, and compare are absent |
| Compare two commits, compare with HEAD, reveal commit | Absent |
| File log | Absent |
| Format Patch | Disabled on a tag |
| Filter the graph by selected refs, recyclable commits | Absent |
| Drag a ref or a commit to rebase or merge | Absent |

### Tools, conflict solver, and AI

| SmartGit command | SannyGit |
| --- | --- |
| Preferences, themes, Git executable, Git config | Present. Interface language is English only |
| Conflict Solver during a merge | Present for text conflicts, including ours, theirs, and recreate |
| Conflict Solver layouts, inner-line apply, whitespace modes, save and find | Absent |
| Continue the same solver through rebase, cherry-pick, and revert | Absent |
| External compare and merge tools | Absent |
| Configurable toolbar and accelerators | Fixed toolbar. The Log toolbar button is disabled even though Log exists on a branch and a tag |
| AI commit message | Present for an OpenAI-compatible server |
| AI reword, explain commit, explain diff, stash description, Git Notes | Absent |
| Garbage collection, output log, proxy, spellcheck | Absent |

### Submodule, subtree, LFS, bisect, review

| SmartGit menu | SannyGit |
| --- | --- |
| Submodule: add, init, deinit, sync, reset, unregister, open | Clone can recurse into submodules. Status lists them. There is no submodule menu |
| Subtree: add, merge, push, reset, split | Absent |
| LFS: install, track, lock, unlock, prune | Absent |
| Bisect: start, good, bad, try, reset | Absent |
| Review: pull requests, merge requests, comments, CI | The Review menu contains one disabled item. No hosting API |
| Git Notes | History reads with `--no-notes` |
| Standard window: My History, feature flow, integrate main, favorite branches | Absent |

## Proposed order

Each phase should leave the current Working Tree usable. A disabled command
stays disabled until its action is real.

### 1. Commands the window already shows

Wire or remove the disabled controls whose meaning is already clear.

- Toolbar Log opens the existing history window for HEAD.
- Repository favorite and rename of a bookmark.
- Rename and delete of a repository group. The header button in Repositories is the place for this.
- Enable Fast-Forward Merge on a tag by opening the merge confirmation that already has that action.
- Copy Message for a tag.
- Open in Terminal stays disabled until phase 5. Launching a system terminal is a separate command, not a control that already has a backend.

Blame, Review, Format Patch, and Pull with rebase stay disabled. They need the later phases.

### 2. Working-tree diff and file commands

This is the largest daily gap with SmartGit's Working Tree.

- Stage, unstage, and discard a hunk and a selected line from the diff.
- Side-by-side and unified modes in the working-tree diff, plus ignore-whitespace.
- Changes view options: group by directory, show only staged or only unstaged, show ignored files.
- Discard and delete for several selected files, still with the review dialog.
- Stash the selection, and rename a stash entry.
- Move or rename a file inside the repository.

Implemented: hunk and line stage, unstage, and discard; unified and side-by-side diffs with ignore-whitespace; directory grouping, staged or unstaged scope, and ignored files; stash of the selection and stash rename; move or rename inside the repository. Open in Terminal, Blame, Review, Format Patch, and Pull with rebase stay disabled.

Index Editor can stay inside the existing diff panel. A separate three-pane HEAD / Index / working-tree window can wait until line staging is in place.

### 3. Operations on commits

SmartGit starts these from the graph, the journal, and the branch menu.

- Context menu on a journal commit and on a local branch: Log, Merge, Rebase, Reset, Reset Advanced, Push, Copy message, Copy id. Implemented. The journal and history window open the existing merge, rebase, reset, and push flows for that commit. Copy Message and Copy ID are also on the local branch menu.
- Cherry-pick and revert of one selected commit, with the same preflight used by merge.
- Rebase Continue, Skip, and Abort while a rebase is in progress. The conflict solver should open for rebase conflicts the way it opens for merge conflicts.
- Squash merge and fast-forward in the main merge dialog.
- Compare the selected commit with HEAD, then compare two selected commits.
- File history for the selected path.

Interactive rebase, reorder, and split stay in phase 6. The current rebase must keep refusing autostash and update-refs until that work has its own design.

### 4. Remote and branch safety

- Pull option to rebase the local branch onto the fetched tip, with the same clean-tree check as Rebase.
- Delete of a local branch that is not merged, behind an explicit confirmation. The current merged-only delete remains the default.
- Push of a reviewed non-fast-forward update only when the user chooses it, using `--force-with-lease` against the reviewed remote oid. Ordinary push stays `--no-force`.
- Set Tracked Branch and Stop Tracking from the local branch.
- Mark a local branch that is checked out in another worktree, and offer to open that worktree instead of switching.

### 5. Repository list and local tools

- Open in Terminal.
- Favorites section and persisted rename of a bookmark.
- Group rename, delete, and reorder.
- Optional external diff and merge tool, configured in Preferences, used by the existing diff and conflict views.

### 6. Repository types SmartGit treats as normal Git

These are separate backends, each with its own tests.

- Submodule add, init, update, and reset of the gitlink. Discard must keep refusing a submodule until this exists.
- Git LFS status and track.
- Git Notes on a commit.
- Bisect start, good, bad, and reset.
- Annotated tags, without enabling `tag.gpgSign` by surprise.
- Interactive rebase from a selected commit: pick, squash, reword, drop, then continue. Preview the plan before Git runs.
- Git-Flow or an equivalent feature, release, and hotfix sequence, after cherry-pick and rebase continue exist.
- Clean of untracked files, with a review list.

### 7. Hosting and the Standard window

SmartGit's Review menu, pull requests, merge requests, CI status, and the Standard window depend on GitHub, GitLab, Bitbucket, or Azure DevOps. SannyGit does not call those APIs. Start this phase only as an explicit project decision. A useful first slice is read-only: detect the hosting provider from the remote URL, open the commit or branch in the browser, and list open pull requests. Creating and merging them is a later slice.

AI explain, reword, and stash descriptions belong with that decision as well. Commit-message generation already exists and should stay on the configured OpenAI-compatible server.

## Not a SannyGit goal

These ship in this SmartGit build and do not belong on the Working Tree roadmap:

- DeepGit blame and origin perspectives
- License registration, on-premise license server, and update server
- Jira commit templates and Gerrit push
- Custom toolbar designer, heap dumps, and the debug menu
- Spellcheck dictionaries and a built-in text editor
- Company-wide installation and a private updater

## Limits that remain until their phase

- History search covers loaded commits only, with a 10,000 commit cap.
- Discard reviews at most 2 MiB and one file.
- Rebase conflicts, cherry-picks, and reverts that are already in progress must be finished outside SannyGit.
- Push does not rewrite remote history.
- Tags created by SannyGit are lightweight.
- The interface language is English.
