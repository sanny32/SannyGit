# Persistence and migrations
SannyGit has not shipped a release yet. Until after the **first public release**,
do **not** add one-off migrations from older on-disk layouts (for example the
monolithic eframe `app.ron` store). Pre-release builds may assume fresh JSON
config under OS-appropriate paths only (`src/persistence.rs`).

After the first release, when changing persisted shapes, paths, or file formats,
add explicit migrations so users upgrading from a released version keep their
settings. Document what each migration reads and writes; keep them reversible or
backed up when practical.

