# Git command diagnostics

The command runner keeps completed process status and stdout separate from
failures. Failures distinguish process startup, I/O, reader/writer thread failure,
nonzero exit, cancellation, timeout and output limits. A completed command's
exit code is retained when available; cancellation and timeout are not reported
as ordinary Git exits. Existing backend/worker interfaces receive a display
message at their boundary.

Control flow uses process status and machine-readable stdout. Localized stderr
is used only for display. For example, `symbolic-ref --quiet HEAD` exiting with
code 1 means detached HEAD; other failures are reported rather than disguised
as detached state. Sync still handles only its existing, explicitly recognized
porcelain push rejection; the diagnostic text does not affect that decision.

## Display policy

Errors go through the existing modal error queue. They include a known Git
subcommand, available exit code and operation-specific recovery hints. There is
no argv dump; failed stdout, commit messages and stdin are excluded. `Debug` output
also excludes raw captured output.

Only an explicit list of local read/query commands may contribute stderr to
a display message. Network, write, hook-capable and unknown commands withhold
their stderr entirely: an arbitrary hook can print a secret without a label.
The process exit code and normal recovery hints remain available.

Read diagnostics hide entire URL/address words, including user information,
paths, query parameters and fragments. Credential markers (including API-key
headers, bearer/password/token fields and private-key blocks) suppress the whole
diagnostic. This scan runs before truncation. Binary output, unsafe controls,
bidirectional controls and an overflowed capture are withheld. Other diagnostic
text is limited to 4 KiB plus a truncation notice. Redaction is deliberately
conservative; it may hide useful text. It is not a general-purpose secret scanner
for arbitrary unlabeled content from a replacement Git executable.

Successful stdout remains machine-readable data or explicitly requested content.
The existing successful worktree-prune preview is unchanged. Diagnostics are
not written to logs or persisted. Output capture limits, process-group timeout
cleanup, credential helpers and SSH configuration are unchanged.

## Verification

Tests cover real exit codes, localized messages, detached-head failure handling,
startup failure, active cancellation, timeout, output limits, URL credentials,
API keys, authorization headers, private keys, hooks, invalid text and truncation.
Modal rendering tests check redacted text, draft retention and stationary panels
in both themes/styles at 1100×700 and 1536×1024. Git writes in tests use temporary
repositories only.
