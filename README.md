# SannyGit

SannyGit is a simple, fast, modern, open-source Git GUI client built with Rust and egui.

## Current status

The project is in its initial UI foundation stage. Repository data and diffs are mock data. Git operations are not implemented yet, and no Git action is performed automatically.

## Requirements

- Rust 1.81 or newer
- A supported desktop platform: Windows, Linux, or macOS

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
