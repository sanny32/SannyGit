# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

Supports repository creation and cloning, status and diff, staging/unstaging,
commit/amend, history, branches and tags, worktrees, fetch, fast-forward-only
pull, and push.

## Build

The build scripts install missing dependencies and produce a Release package:

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

Packages are written to Cargo's target directory under `package/`. Default install
locations are `~/Applications` on macOS, `~/.local` on Linux and
`%LOCALAPPDATA%\Programs\SannyGit` on Windows. Add `~/.local/bin` to `PATH` on Linux.

## MIT License

Copyright 2026 Alexandr Ananev [mail@ananev.org]

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
