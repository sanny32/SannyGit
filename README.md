# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit. Under development.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

Supports repository creation and cloning, status and diff, staging/unstaging,
commit/amend, history, branches and tags, worktrees, fetch, fast-forward-only
pull, and push. Git operations run in background workers.

Commit and Amend currently stage and include **all changes**, including untracked
files, regardless of the current staging selection. Draft commit messages are
not restored after restarting. Discard, stash, merge/rebase, conflict resolution,
partial staging, and pushing tags are not implemented. History is limited to
500 commits; large status results and diffs have display limits.

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

Commit messages can be generated through an OpenAI-compatible API configured
under **Preferences → AI → Commit messages**. Review the generated draft before
explicitly creating the commit.

For local generation, select the Ollama (`http://localhost:11434/v1/chat/completions`)
or LM Studio (`http://localhost:1234/v1/chat/completions`) endpoint preset. Start
the local server separately and enter the exact name of an installed model;
leave the API key empty unless your server requires authentication. SannyGit
does not download or start models. Local requests allow up to five minutes
for model loading and generation.

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

GitHub Actions runs clippy, tests, and builds on Windows, Linux, and macOS;
formatting and a separate `cargo check --all-targets` with Rust 1.95.0 run on Linux.
Since `Cargo.lock` is not
tracked, fresh checkouts resolve compatible dependencies. Tests use temporary Git
repositories and loopback HTTP servers; no external service or credentials are
required. A sandbox must allow local sockets to run the complete test suite.

## License

[MIT](LICENSE)
