# Preview release verification

Phase D is in progress. This is an evidence log and repeatable checklist, not a
release approval. Publishing, tags, distribution signing and notarization require
separate explicit actions.

## Candidate and local evidence

Checked on November 11, 2026, from commit `4cbcac6` (version `0.1.0`). The pending
changes fix the Windows installer workflow's outdated NSIS input path and add
existing build-script/entitlement tests to macOS CI. No Rust source changed.

Environment: macOS arm64, Rust/Cargo 1.99.1, Apple Git 2.54.0. This was an existing
development machine, not a clean installation or a Rust 1.95 compatibility test.

| Check | Result |
| --- | --- |
| Release build using `./build.sh --install=<temporary directory>` | Passed; prefix contains spaces |
| Bundle identifier, version, executable, icon and both licenses | Passed: `io.github.sanny32.SannyGit`, `0.1.0`, arm64 Mach-O |
| `plutil -lint` and `codesign --verify --deep --strict` | Passed for staged bundle and bundle inside mounted DMG |
| Dynamic dependencies | `otool -L` lists only Apple system frameworks/libraries; no development-machine library paths |
| DMG creation and `hdiutil verify` | Passed; image mounted read-only, contents compared with staging, then detached |
| Packaged application's native launch | Passed; the installed temporary bundle was accessible through computer-use tools |
| GUI init → edit → stage → commit → push → clone → pull | Passed in Modern style with system theme resolving to light, using disposable local repositories |
| Paths with spaces and Unicode | Passed for repository directories and `пример file.txt` |
| Git verification after GUI operations | HEAD matched the local bare remote and clone; expected file bytes and clean index/worktree after pull |
| Dark theme, normal window | Visually inspected main panels, diff and error modal; separate Fetch and retry exercised |
| Locked index | Stage failed with modal error; draft, file bytes and index preserved |
| Unavailable local transport | Fetch against a missing local remote failed with modal error; same dialog allowed retry after remote restoration |
| Build/install script tests | 8 passed; Linux behavior is simulated on macOS, not a native Linux test |
| macOS entitlement derivation tests | 6 passed; no real credential or provisioning-profile round trip |
| Build script syntax, modified workflow YAML, `git diff --check` | Passed |

The GUI created the first commit and pushed/cloned it. A second commit was then
prepared and pushed by the test fixture in its temporary working repository;
the GUI fetched and fast-forwarded the clone to that exact commit. No test commit,
staging, Git configuration change or push targeted the SannyGit working repository
or a real server. The error checks used an owned temporary lock and missing local
path, not a real network outage.

The smoke fixture is under a disposable `/private/tmp/SannyGit smoke Юникод ...`
directory. The app retains its two test repository bookmarks because bookmark
removal is currently disabled. The original repository selection and automatic
theme are restored after inspection. Keep the fixture while those bookmarks are
needed; do not remove unrelated temporary directories.

## Local artifact

Generated file (ignored build output):
`target/phase-d/sannygit-0.1.0-macos-arm64.dmg`.
Its sibling `SHA256SUMS` contains:

```text
2de4ed8ce713ea885ab4f39ba33098db6a6672826a38044ad317b39bfb7d0d2e  sannygit-0.1.0-macos-arm64.dmg
```

This is an ad-hoc signed local test artifact. It is not notarized, distribution
signed, uploaded, or verified on a clean Mac. Without a matching provisioning
profile it uses login Keychain storage rather than Touch ID storage. Git must
already be available on the destination machine.

To reproduce the build, use a fresh temporary staging directory and the same
`build.sh --install=<stage>` invocation as the macOS packaging workflow. Verify
the bundle, add an `Applications` symlink in the staging directory, then create
and verify a DMG as described in
[the macOS workflow](../.github/workflows/build-installer-macos.yaml).
Record the source commit, architecture, tool versions, checksum and any local
changes with each new artifact; a rebuilt image may have a different checksum.

## Platform and release gates

A checked box below requires evidence for the packaged application on the actual
platform. Automated egui/backend tests and package extraction alone do not close
these gates.

| Target | Format | Current evidence | Remaining gate |
| --- | --- | --- | --- |
| macOS arm64 | DMG | Local build, signature/integrity/content checks; partial native GUI smoke above | Clean-Mac install/launch and full theme/size/credential coverage |
| macOS Intel | DMG if supported | None from this session | Decide architecture support and verify on an Intel Mac; current workflow uses runner architecture |
| Windows x64 | NSIS installer | Outdated script path fixed locally | Hosted build, install/uninstall and native workflow checks |
| Linux Debian/Ubuntu x86_64 | DEB | Workflow and metadata inspected | Native package build, dependency installation and X11/Wayland GUI checks |
| Linux Fedora x86_64 | RPM | Workflow and metadata inspected | Native package build/install and GUI checks |
| Linux x86_64 | AppImage | Workflow inspected | Extraction and launch on a clean system, with and without FUSE where supported |

### Native workflow, for each supported OS

- [ ] Use disposable repositories and a local bare remote to repeat init/clone,
  edit, stage/unstage, commit/amend, push, fetch and fast-forward pull. Verify refs,
  index and working-file bytes after each operation.
- [ ] Repeat the complete workflow in light/dark themes and Classic/Modern styles,
  at the normal window size and minimum content size of 1100 × 700. The current
  GUI tool did not resize the native macOS window when its corner was dragged;
  exact minimum-size native verification remains open.
- [ ] Verify error/retry/cancel preserves rows, scroll position, selection, draft
  text and dialog geometry through progress, failure and recovery.
- [ ] Check hooks and commit signing with disposable identities, including failure;
  check conflicts and external Git-operation guards without losing work.
- [ ] Check real HTTPS credential helpers and SSH agents against an explicitly
  authorized test remote, including authentication failure and cancellation.
- [ ] Check system credential storage across restart/update with the intended
  signing identity. Verify Touch ID separately with an authorized profile.
- [ ] Check keyboard-only operation, focus, window size/position restoration,
  DPI changes, multiple monitors and OS-native title-bar controls.
- [ ] On Linux, check both Wayland and X11; record distribution and library versions.

### Distribution

- [ ] Run checks and packaging workflows for the exact candidate revision; record
  run URLs and per-platform results. Do not substitute an earlier successful run.
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

Outstanding checks are not release-ready. This session did not verify native
Windows/Linux, clean systems, real HTTPS/SSH authentication, native Keychain/Touch
ID, distribution signing, exact native minimum-size layout, DPI/multiple monitors,
or full keyboard/restart coverage. Existing commit-content, history and diff
limits remain documented in the main project documentation; merge/rebase and
partial staging remain outside this preview's implemented workflow.
