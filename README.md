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

## License

[MIT](LICENSE)
