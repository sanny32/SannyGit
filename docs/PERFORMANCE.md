# Backend performance measurements

C3 baseline, November 11, 2026. These are local measurements, not release
performance guarantees or CI timing thresholds.

## Reproduce

```sh
cargo test backend_measurements -- --ignored --nocapture --test-threads=1
```

The ignored test creates and removes three temporary repositories. It never
opens the user's repositories, accesses credentials or contacts a server.
It prints JSON records with sample count, minimum/median/maximum wall time,
and observed Git process counts. The default test suite skips this benchmark.
Use `cargo test --release backend_measurements -- --ignored --nocapture --test-threads=1`
to measure an optimized build separately. Avoid concurrent builds and tests.

Fixtures contain 100 files/100 commits, 1,000 files/1,000 commits, and
5,000 files/10,000 commits. Each has a linear history and a local upstream one
commit behind HEAD. The root commit adds every file; subsequent commits change
one file. Dirty fixtures modify 1% of tracked files and add the same number of
untracked files. The selected file has both staged and unstaged changes, each
with 1,000 lines. Git hooks, signing and automatic fetch are disabled.

Every timed operation has one warm-up and five measured samples. Timings cover
the application backend, including Git process startup, waiting, parsing and
history graph preparation. Diff timings also include `DiffPreview` construction.
They exclude fixture construction and GUI rendering. Counts observe successful
Git launches through the backend for the fixture path, including worker threads;
they exclude fixture setup and subprocesses launched internally by Git.
The counter and benchmark helpers are compiled only for tests.

## Local results

Environment: macOS 27.2.1, aarch64, Rust/Cargo 1.99.1, Git 2.54.0
(Apple Git-157), debug test profile. History pages contain up to 500 commits.
The small fixture has only 100 commits and has no second page.
Application baseline: `9201b47`, plus the test-only C3 measurement harness.

Median wall time in milliseconds, with minimum–maximum in parentheses. Each
process count is per operation; fixture size is files/commits.

| Operation | 100 / 100 | 1,000 / 1,000 | 5,000 / 10,000 | Git processes |
| --- | ---: | ---: | ---: | ---: |
| Working-tree snapshot, clean | 76.0 (72.7–77.1) | 75.7 (74.4–100.4) | 100.1 (99.3–100.7) | 3 |
| Working-tree snapshot, dirty | 74.4 (71.0–77.2) | 74.8 (68.5–76.8) | 74.5 (73.2–77.5) | 3 |
| Upstream tracking | 74.4 (72.8–75.7) | 72.4 (69.1–74.2) | 75.6 (74.1–75.9) | 3 |
| Selected working-file diff | 48.0 (46.6–53.1) | 51.0 (49.0–53.2) | 51.6 (50.0–53.1) | 2 |
| First history page | 447.5 (440.3–452.8) | 447.0 (435.6–744.4) | 499.6 (483.4–500.7) | 18 |
| Next history page | — | 226.2 (219.5–235.6) | 232.1 (228.0–235.0) | 9 |
| Root commit file list | 24.2 (21.1–26.2) | 25.9 (21.2–26.1) | 26.4 (23.3–26.6) | 1 |
| Selected root-commit file diff | 22.7 (21.9–25.9) | 25.7 (24.0–26.0) | 25.6 (24.2–25.8) | 1 |

| Observed polling | 100 / 100 | 1,000 / 1,000 | 5,000 / 10,000 |
| --- | ---: | ---: | ---: |
| Clean tree, processes/minute | 119.9 (24 in 12.01 s) | 119.9 (24 in 12.01 s) | 119.9 (24 in 12.01 s) |
| Dirty tree, processes/minute | 169.8 (34 in 12.01 s) | 170.0 (34 in 12.00 s) | 169.9 (34 in 12.00 s) |

## Polling and next steps

Each clean and dirty fixture is observed for 12 seconds through the actual
working-tree and upstream monitors, driven every 10 ms without rendering.
Pending reads are drained before reporting. Reported processes/minute are
extrapolated from these short windows and include the initial upstream read;
they are not a one-minute native-app recording. Auto-fetch and interactive
history loading are excluded.

Working-tree snapshots use three Git processes (root, branch, status); a
selected tracked-file diff adds two. Tracking uses three in this fixture
(refs, ahead/behind counts, outgoing filenames). Initial history loading uses
18 processes, including snapshot validation; the next page uses nine. Commit
file-list and selected-file patch reads each use one.

The measurements establish a baseline for later comparisons. They do not
demonstrate a need for a filesystem watcher or justify increasing external-edit
detection latency with idle backoff. Keep the current polling intervals for
now. If profiling real repositories shows material CPU/I/O cost, compare a
combined root/branch/status read against this baseline before changing polling.
History snapshot validation must continue to detect concurrent HEAD/ref changes.
The command runner polls completion at 20 ms intervals, so these wall times
include waiting overhead as well as Git's actual work.

Cold-cache behavior, CPU usage, memory, native frame latency, large merges,
network filesystems, actual auto-fetch and Windows/Linux are not measured here.
Those require separate measurements before generalizing these results.
