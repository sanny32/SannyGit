# SannyGit

SannyGit is a simple, fast, modern, open-source Git GUI client built with Rust and egui.

## Current status

The project is in its initial UI foundation stage. Repository data and diffs are mock data. Git operations are not implemented yet, and no Git action is performed automatically.

## Requirements

- Rust 1.95 or newer
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
