# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

Supports:

- Repository creation and cloning
- Status and diff
- Staging/unstaging
- Single-file discard
- Stash save/apply/pop/drop
- Commit/amend
- History
- Branches and tags
- Merged-branch deletion
- Worktrees
- Remote URL management
- Fetch
- Fast-forward-only pull
- Branch/tag push

History loads in pages of 500 commits, up to 10,000. Search covers loaded history.

## Build

The build scripts install missing dependencies and build in Release mode:

```sh
./build.sh                       # Linux / macOS
./build.sh --tests               # Include tests
./build.sh --install             # Build and install
./build.sh --install=/path       # Use a custom install prefix
```

On Windows:

```powershell
.\build.ps1
.\build.ps1 -Tests -Install
.\build.ps1 -InstallPrefix "C:\Apps\SannyGit"
```

Linux dependency setup uses apt, dnf, zypper or pacman and may require sudo.
macOS requires Xcode Command Line Tools; missing tools use Homebrew or rustup.
Windows dependency setup requires winget and may request elevation.

For macOS Keychain access across updates, sign each build with the same certificate:
`./build.sh --sign="Certificate name or SHA-1 identity" --install`. The certificate
must already be installed in Keychain. Ad-hoc builds, including `cargo run`, may
trigger two Keychain authorization dialogs even for a single API-key read. An
existing key may need authorization again when changing the signing identity.

For Touch ID, also pass `--keychain-profile=/path/SannyGit.provisionprofile`
with a matching macOS provisioning profile for `io.github.sanny32.SannyGit`
that permits Keychain access. Re-enter and save an existing API key once in
this signed build. Unlocked keys stay in memory until the app closes.
Use `--debug` for a development bundle that supports debugger attachment.

Default install locations are `~/Applications` on macOS, `~/.local` on Linux and
`%LOCALAPPDATA%\Programs\SannyGit` on Windows. Add `~/.local/bin` to `PATH` on Linux.

## Packages

Separate GitHub Actions workflows create macOS DMG, Windows installer, Linux DEB,
RPM and AppImage packages. Run them manually or push a `v<version>` tag matching
`Cargo.toml`. Packages and SHA-256 checksums are available in the workflow's
artifacts. Packages are not signed with a distribution certificate or notarized.

## MIT License

Copyright 2026 Alexandr Ananev [mail@ananev.org]

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
