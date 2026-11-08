# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit. Under development.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

## Run

```sh
cargo run
```

Commit messages can be generated through an OpenAI-compatible API configured
under **Preferences → AI → Commit messages**. Review the generated draft before
explicitly creating the commit.

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

## License

[MIT](LICENSE)
