# Git Backend
These rules apply when extending the existing backend:

- Start with the installed Git executable through `std::process::Command`.
  Pass arguments separately; do not construct shell commands from user input.
- Separate command execution and output parsing from the UI. Do not introduce
  `git2` ahead of an actual need.
- Use machine-readable formats, such as `status --porcelain=v1 -z`,
  `diff --no-ext-diff`, and explicit `log` and `branch` formats.
  Do not parse localized, human-readable output.
- Run Git commands and expensive operations on background threads, passing
  results to the UI through channels. Do not execute commands on every egui frame.
- Provide operation status, clear errors, and limits on loaded diffs and history
  so the interface remains responsive.
- Use existing Git credential helpers and SSH configuration. Never store
  passwords or tokens in plain text or write them to logs.
- Do not add GitHub/GitLab APIs, PR management, AI features, plugins, a terminal,
  a code editor, cloud synchronization, or an advanced rebase UI without a
  separate task.

