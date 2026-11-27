# Preview release verification

Phase D is in progress. This is an evidence log and repeatable checklist, not a
release approval. Publishing, tags, distribution signing and notarization require
separate explicit actions.

Last updated on November 27, 2026, for version `1.0.0` at commit `8d64837`. No
native package build, installer run or GUI smoke test has been recorded for this
candidate.

## Current candidate

- Revision: `8d64837` (Enable deleting unmerged local branches after confirmation).
- Version: `1.0.0`.
- Native build, install and GUI checks for this revision: not performed.
- Required Git version: 2.36 or newer on `PATH`.

The checks marked as outstanding apply to this revision until a later entry records
actual evidence.

## Scope requiring verification

These areas are part of this candidate and have no native verification recorded:

- Git operations: merge (create-commit, fast-forward, squash, merge to working
  tree, abort), rebase with Continue/Skip/Abort and the text conflict solver,
  cherry-pick and revert with Continue/Skip, reset (soft, mixed, hard), pull with
  merge or rebase, push up to a tracked branch, and deletion of an unmerged local
  branch after an explicit confirmation.
- Working tree: hunk and line staging/unstaging/discard, side-by-side diff,
  ignore-whitespace, ignored files, move or rename inside the repository, staged
  or unstaged scope, directory grouping, stash of the selection, and stash rename.
- Commit message generation: OpenAI-compatible endpoints, local servers
  (Ollama, LM Studio), ChatGPT subscription sign-in, GitHub Copilot sign-in,
  provider and connection state, configurable context, token usage display,
  cancellation, and deferred credential loading. Provider calls use direct API
  requests; the ChatGPT callback is served as a local page.
- Settings and persistence: JSON files under platform data directories replacing
  the monolithic eframe store, per-repository last branch, repository favorites
  and rename, confirmation toggles, appearance dropdowns, and shortcut overrides.
  Pre-release builds intentionally do not migrate settings from older on-disk
  layouts.
- Native integration: modal child windows on Windows and Linux, the patched
  winit mixed-DPI drag fix, Metal rendering on macOS, system theme following,
  dynamic window title, and the native macOS menu.
- Packaging and CI: a shared `project-version` action with tag/version checking,
  Cargo caching, silent Windows install/uninstall verification, macOS bundle and
  DMG verification, and DEB/RPM/AppImage content checks.

## Platform and release gates

A checked box below requires evidence for the packaged application on the actual
platform. Automated egui/backend tests and package extraction alone do not close
these gates.

| Target | Format | Current evidence | Remaining gate |
| --- | --- | --- | --- |
| macOS arm64 | DMG | No evidence for this revision. Workflow builds, lints the plist, verifies signature and licenses, and creates/verifies a DMG; it does not launch the app. | Run the workflow for `8d64837`; clean-Mac install/launch; full theme/style/size/credential coverage |
| macOS Intel | DMG if supported | No evidence. The workflow uses the runner architecture (`macos-latest` is arm64). | Decide architecture support and verify on an Intel Mac |
| Windows x64 | NSIS installer | No evidence for this revision. Workflow stages with `build.ps1`, validates the installed file set, and checks that silent uninstall keeps an unrelated user file. | Hosted run for the candidate, native install/uninstall, GUI workflow checks |
| Linux Debian/Ubuntu x86_64 | DEB | No evidence for this revision. Workflow derives dependencies with `dpkg-shlibdeps`, validates the desktop file and verifies extracted contents and licenses. | Native package build/install, dependency resolution and X11/Wayland GUI checks |
| Linux Fedora x86_64 | RPM | No evidence for this revision. Workflow builds in a Fedora container, validates the desktop file, and checks package name and version while reporting the file list and requirements. | Native package build/install and GUI checks |
| Linux x86_64 | AppImage | No evidence for this revision. Workflow builds with linuxdeploy, bundles the winit runtime libraries, then extracts with `--appimage-extract` and verifies the binary and licenses. | Launch on a clean system, with and without FUSE where supported |

### Native workflow, for each supported OS

- [ ] Use disposable repositories and a local bare remote to repeat init/clone,
  edit, stage/unstage, commit/amend, push, fetch and fast-forward pull. Verify refs,
  index and working-file bytes after each operation.
- [ ] Exercise merge, rebase, cherry-pick, revert and reset, including conflicts,
  Continue/Skip/Abort, the text conflict solver, and abort leaving the worktree
  clean. Exercise pull with merge and rebase, push up to a tracked commit, and
  deletion of an unmerged local branch; confirm Cancel keeps the branch and that
  the current and worktree branches stay protected.
- [ ] Stage, unstage and discard a hunk and a selected line; move or rename a file;
  filter staged/unstaged/ignored files; verify diff inspection, review dialogs and
  cancellation preserve drafts, rows, scroll position and selection.
- [ ] Repeat the complete workflow in light/dark themes and Classic/Modern styles,
  at the normal window size and minimum content size of 1100 × 700.
- [ ] Verify error/retry/cancel preserves rows, scroll position, selection, draft
  text and dialog geometry through progress, failure and recovery.
- [ ] Check hooks and commit signing with disposable identities, including failure;
  check conflicts and external Git-operation guards without losing work.
- [ ] Check real HTTPS credential helpers and SSH agents against an explicitly
  authorized test remote, including authentication failure and cancellation.
- [ ] Check system credential storage across restart/update with the intended
  signing identity. Verify Touch ID separately with an authorized profile.
- [ ] Configure commit message generation for an API key, ChatGPT subscription,
  GitHub Copilot and a local server. Verify browser sign-in and the local callback
  page, denied/cancelled/expired authorization, provider availability, cancelled
  generation, restored credentials after restart, the token-count setting, and
  that no credential or token reaches logs or the working tree.
- [ ] Check settings persistence in a fresh profile: JSON files under the platform
  data directory, sane file permissions, missing/corrupt/read-only file handling,
  restore to defaults, and behavior across restart. Do not expect migration from
  older pre-release development layouts.
- [ ] Check worktree add/remove/prune and the guards for branches checked out in
  another worktree.
- [ ] Check keyboard-only operation, focus, window size/position restoration,
  DPI changes, multiple monitors and OS-native title-bar controls. Verify the
  patched mixed-DPI window drag on Windows and modal child windows on Windows and
  Linux.
- [ ] On Linux, check both Wayland and X11; record distribution and library
  versions. On macOS, confirm Metal rendering and the native menu.

### Distribution

- [ ] Run checks and packaging workflows for the exact candidate revision; record
  run URLs and per-platform results. Do not substitute an earlier successful run.
  A release tag must match the Cargo version (`v1.0.0`); the shared
  `project-version` action fails the package job on a mismatch.
- [ ] Download each artifact and verify its published SHA-256 checksum.
- [ ] On clean systems, verify install/launch, application identity, icons, licenses,
  Git availability and runtime dependencies. Check Windows uninstall preserves
  unrelated user files and preferences.
- [ ] Test upgrade/reinstall and OS security prompts. Record signing/notarization
  status and required user steps; do not bypass security prompts during testing.
- [ ] Record supported OS versions/architectures and known limitations from actual
  results before preparing release notes. Current runner labels alone do not
  establish the minimum OS version.
- [ ] Obtain explicit authorization before publishing or creating release tags.

Outstanding checks are not release-ready. No native verification exists for `1.0.0`
on any platform. Native Windows/Linux, clean systems, real HTTPS/SSH
authentication, native Keychain/Touch ID, distribution signing, exact native
minimum-size layout, DPI/multiple monitors, AI provider round trips and full
keyboard/restart coverage remain unverified. Existing limits still apply: history
search covers loaded commits only with a 10,000 commit cap; discard reviews one
file up to 2 MiB; interactive rebase, annotated tags and force push are absent;
submodule, LFS and hosting-provider features are not implemented; the interface is
English only.
