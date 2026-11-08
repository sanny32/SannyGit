# SannyGit

A desktop Git client for Windows, Linux and macOS, built with Rust and egui and inspired by SmartGit. Under development.

Requires Rust 1.95+ and Git 2.36+ on `PATH`.

## Run

```sh
cargo run
```

Commit messages can be generated from the staged diff through an
OpenAI-compatible chat completions API. Configure the endpoint, model,
instructions, and optional API key under **Preferences → AI → Commit messages**.
The API key is kept only in memory; set `SANNYGIT_LLM_API_KEY` to provide it when
the application starts. Review the generated draft before explicitly creating
the commit.

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
