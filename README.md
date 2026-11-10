# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

Supports repository creation and cloning, status and diff, staging/unstaging,
commit/amend, history, branches and tags, worktrees, fetch, fast-forward-only
pull, and push.

## Run

Install Rust through [rustup](https://rustup.rs/) and a native build toolchain
(Xcode Command Line Tools on macOS, MSVC Build Tools with the Windows SDK on
Windows, or a C/C++ toolchain on Linux).

On Ubuntu 24.04, install the Linux build dependencies:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libdbus-1-dev libwayland-dev libxkbcommon-dev libx11-dev libxrandr-dev libxi-dev libxcursor-dev libgl1-mesa-dev libxcb1-dev libxcb-randr0-dev
```

From the repository directory:

```sh
cargo run
```

Optional commit-message generation through an OpenAI-compatible API or local
Ollama/LM Studio server is configured under **Preferences → AI → Commit messages**.

## Package

To build an app bundle/package with its icon:

```sh
python3 scripts/package.py
```

## Check

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Tests use temporary Git repositories and local HTTP servers. No external service
or credentials are required; local sockets must be allowed.

## License

[MIT](LICENSE)
