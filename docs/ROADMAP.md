# SannyGit roadmap compared with SmartGit

Compared on 2026-11-22. Parity audit for implemented phases: 2026-11-23.

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
| Merge | Create merge-commit, merge into the working tree, abort, text conflict solver. Fast-forward is offered when merging a tag or another branch |
| Cherry-pick and revert | One commit from the journal or history window; abort and commit-on-success like merge; no Continue/Skip in the UI |
| Rebase | Replay the current clean branch onto one selected commit. Conflicts are finished outside SannyGit |
| Commit menu / push up to | Log, merge, rebase, reset, push up to on tracked branch, copy message and id on journal and history commits |
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
| Mark as Favorite, Rename | Present. Persisted in repository preferences |
| Open in Terminal, Open in VS Code | Terminal row is disabled. There is no editor command |
| Select Obsolete Repositories | Absent |
| Group rename, delete, and reorder | Rename and delete exist. Reorder groups is absent |

### Local and Changes

| SmartGit command | SannyGit |
| --- | --- |
| Stage, Unstage, Commit, Ignore | Present for whole files |
| Discard | One file or symlink. Directories, submodules, and conflicts are refused |
| Stash All / Apply / Drop | Present, together with pop |
| Stash Selection, Rename Stash | Present |
| Index Editor: stage, unstage, or discard a hunk, line, inner-line change, or selection | Partial. Hunk and line stage, unstage, and discard in the working-tree diff. Inner-line and intra-line selection are absent |
| Changes view: side by side, compact, whitespace, line endings, recursive directories, separate index and working tree, show unchanged, ignored, or skipped files | Partial. Side-by-side and ignore-whitespace in the working-tree diff; directory grouping; staged-only or unstaged-only scope; show ignored. Compact layout, line endings, separate index and working tree panes, unchanged and skip-worktree are absent |
| Clean Working Tree | Absent |
| Move or Rename | Present inside the repository from the changes list |
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
| Continue, Skip, Abort, Step during rebase, cherry-pick, or revert | Partial. Abort exists for merge, cherry-pick, and revert. Cherry-pick and revert can finish with Commit after the solver. Continue, Skip, and Step are absent; rebase still directs the user to a terminal |
| Cherry-Pick, Revert | Present from a journal or history commit menu without a confirmation dialog |
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
| Fetch All, Fetch More, Push Up To | Push Up To is present from a commit menu on the tracked branch. Fetch All and Fetch More are absent |
| Synchronize as pull-then-push and push-then-pull | Sync is push, then a fast-forward pull |
| Remote rename as a first-class command, connection properties | URLs can be edited. Renaming the remote is not a separate command |
| Push to Gerrit | Absent |

### Query, journal, and log

| SmartGit command | SannyGit |
| --- | --- |
| Log, graph, current branch and all branches | Present, with the load limits above |
| Journal: incoming, outgoing, and rewritten commits | Ahead/behind tracking exists. The journal does not color incoming, outgoing, or rewritten commits, and it has no second comparison branch |
| Blame | Menu item is disabled |
| Commit context menu: Log, Merge, Rebase, Reset, Reset Advanced, Push, Copy Message, Copy ID | Present on a journal commit and in the history window, together with Cherry-Pick and Revert. Push from a commit fast-forwards the current branch’s tracked remote branch up to that commit. Checkout, new branch, and compare are absent |
| Compare two commits, compare with HEAD, reveal commit | Absent |
| File log | Absent |
| Format Patch | Present on a tag when repository data is current |
| Filter the graph by selected refs, recyclable commits | Absent |
| Drag a ref or a commit to rebase or merge | Absent |

### Tools, conflict solver, and AI

| SmartGit command | SannyGit |
| --- | --- |
| Preferences, themes, Git executable, Git config | Present. Interface language is English only |
| Conflict Solver during a merge | Present for text conflicts, including ours, theirs, and recreate |
| Conflict Solver layouts, inner-line apply, whitespace modes, save and find | Absent |
| Continue the same solver through rebase, cherry-pick, and revert | Partial for cherry-pick and revert, same as merge. Rebase conflicts still have no in-app solver |
| External compare and merge tools | Absent |
| Configurable toolbar and accelerators | Fixed toolbar. The Log toolbar button focuses Journal on HEAD; branch and tag Log still open the history window |
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

## Parity audit: mismatch and required work

This table compares **implemented or claimed** Working Tree behavior with SmartGit
build 261056. It is the actionable follow-up to phases 1–3. The **Gap by SmartGit
menu** section above is updated when parity changes; this section records what still
differs and where to fix it.

| Area | SmartGit | SannyGit today | Required work | Where |
| --- | --- | --- | --- | --- |
| Toolbar Log | Opens the Log window on HEAD | **Done:** `open_current_branch_log` on the current local branch (detached HEAD falls back to `open_head_log`) | — | Phase 1 |
| Repository favorites | Dedicated favorites list in the repositories UI | Favorite flag is stored; list is not separated from other bookmarks | Add a favorites section and keep rename persistence | Phase 5 |
| Repository groups | Rename, delete, reorder groups | Rename and delete work; reorder is missing | Implement drag or explicit reorder for groups | Phase 5 |
| Index Editor | Hunk, line, inner-line, and selection | Hunk and line only | Inner-line apply and explicit selection staging if SmartGit parity is required | Phase 2 extension or phase 6 |
| Changes view | Compact, line endings, index vs working tree, unchanged, skip-worktree | Grouping, scope, ignored, side-by-side, ignore-whitespace | Add remaining view options or document deliberate omissions | Phase 2 extension |
| Discard | Directories, submodules, conflicts outside an active merge | Single file or symlink, 2 MiB cap | Extend discard with review, or keep refusing until submodule support | Phase 6 for submodules |
| Main Merge dialog | Fast-forward and squash beside create commit and merge to working tree | **Done:** Fast-Forward and Squash Merge in the main picker (`merge::Mode`) | Validate FF with Git at run time; squash conflicts still use merge flow | Phase 3 |
| Rebase in progress | Continue, Skip, Abort, conflict solver | **Partial:** Continue, Skip, Abort on the Changes banner; `Sequencing::Rebase` enables the conflict solver like merge | Step command; richer rebase UI | Phase 3 |
| Cherry-pick / Revert | Confirm optional; Continue and Skip; revert merge with parent choice | **Done:** Continue/Skip on banner; merge-commit revert parent dialog; **no** optional confirmation yet | Optional confirmation dialog | Phase 3 |
| Cherry-pick preflight | Same safety checks as merge | **Done:** `git::preflight_apply_commit` | — | Phase 3 |
| Commit menu parity | Checkout, new branch, compare with HEAD, compare two commits | **Done:** Check Out, New Branch…, Compare with HEAD, comparison commit and two-commit compare | — | Phase 3 |
| File history | File log from selection | **Done:** Log on a changed file opens a history window with `git rev-list --follow` | — | Phase 3 |
| Journal presentation | Incoming, outgoing, rewritten coloring; second comparison branch | Ahead/behind in status; plain graph | Color journal rows; optional comparison ref | Later UX, not in current phases |
| Remote push | Push Up To from ref and commit menus | Push Up To from commit menu on tracked branch only | Broader Push Up To entry points if SmartGit exposes more | Phase 4 |
| Pull | Merge or rebase pull | **Done:** FF merge or rebase on Pull dialog; clean-tree check unchanged | — | Phase 4 |
| Force push | Explicit reviewed non-FF push | `--no-force` everywhere | `--force-with-lease` when user confirms | Phase 4 |
| Format Patch / Pull rebase in menu | Available where SmartGit enables them | Format Patch on tags works; Pull rebase enabled in options | — | Phase 5 |
| Roadmap limits text | — | Still says cherry-pick and revert must finish outside SannyGit | Update limits once Continue/Skip and rebase solver ship | This file, **Limits** below |

### Phase completion notes (claimed vs SmartGit)

| Phase | Claimed | Still missing for SmartGit parity |
| --- | --- | --- |
| 1 | Toolbar Log, favorites, groups, tag FF, copy tag message | Toolbar Log behavior; group reorder; favorites section (phase 5) |
| 2 | Index editor and changes options | Inner-line, compact, line endings, index/working panes, skip-worktree |
| 3 | Journal commit menu; cherry-pick and revert | Continue/Skip; revert merge parent; main-dialog FF/squash; rebase Continue/Skip/Abort and solver; compare and file log; commit checkout and new branch |

## Proposed order

Each phase should leave the current Working Tree usable. A disabled command
stays disabled until its action is real.

### 1. Commands the window already shows

Wire or remove the disabled controls whose meaning is already clear.

- Toolbar Log opens the existing history window for HEAD. **Remaining:** toolbar still reveals Journal only; align with branch Log (`open_ref_log` on current branch).
- Repository favorite and rename of a bookmark. **Implemented** (persisted). Favorites section waits for phase 5.
- Rename and delete of a repository group. The header button in Repositories is the place for this. **Implemented.** Reorder waits for phase 5.
- Enable Fast-Forward Merge on a tag by opening the merge confirmation that already has that action. **Implemented.**
- Copy Message for a tag. **Implemented.**
- Open in Terminal stays disabled until phase 5. Launching a system terminal is a separate command, not a control that already has a backend.

Blame, Review, and Pull with rebase stay disabled until their phases. Format Patch on tags is enabled when repository data is current.

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

- Context menu on a journal commit and on a local branch: Log, Merge, Rebase, Reset, Reset Advanced, Push, Copy message, Copy id. Implemented. Log from the journal opens the current branch history on that commit. Push from a commit fast-forwards the current branch’s tracked remote branch up to that commit; rewriting the remote branch stays in phase 4. The local branch menu still pushes that branch. Copy Message and Copy ID are also on the local branch menu.
- Cherry-pick and revert of one selected commit, with the same preflight used by merge. **Partial.** Journal and history commit menu; conflicts use the merge commit flow and Abort on the changes banner. **Remaining:** untracked collision preflight, Continue/Skip, revert of merge commits with parent selection; optional confirmation dialog.
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
- Rebase conflicts must be continued or aborted outside SannyGit until phase 3 adds Continue, Skip, and Abort.
- Cherry-pick and revert can be aborted, continued, or skipped from the Changes banner, and completed with Commit after conflicts.
- Rebase can be aborted, continued, or skipped from the Changes banner; the text conflict solver applies while `Sequencing::Rebase` is active.
- Push does not rewrite remote history.
- Tags created by SannyGit are lightweight.
- The interface language is English.
