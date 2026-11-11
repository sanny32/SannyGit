//! Bounded history reads and graph preparation, executed by background workers.
use crate::{
    git,
    models::{Commit, History, HistoryEdge},
};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const PAGE_SIZE: usize = 500;
pub const HISTORY_LIMIT: usize = 10_000;
const REF_LIMIT: usize = 5_000;

#[derive(Clone, PartialEq, Eq)]
struct Basis {
    head: String,
    refs: Vec<u8>,
    shallow: Vec<u8>,
    grafts: Vec<u8>,
}

#[derive(Clone)]
pub struct Cursor {
    basis: Basis,
    all_branches: bool,
    ids: Vec<String>,
    next: usize,
    capped: bool,
    page_size: usize,
}

impl Cursor {
    pub fn has_more(&self) -> bool {
        self.next < self.ids.len()
    }
    pub fn loaded(&self) -> usize {
        self.next
    }
    pub fn all_branches(&self) -> bool {
        self.all_branches
    }
    pub fn at_limit(&self) -> bool {
        self.capped && !self.has_more()
    }
}

pub struct Page {
    pub history: History,
    pub cursor: Cursor,
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("History loading cancelled".into())
    } else {
        Ok(())
    }
}

fn run(root: &Path, args: &[&str], cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    git::run_controlled(root, args, Duration::from_secs(15), Some(cancel))
}

fn boundary(root: &Path, name: &str, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let path = git::text(run(root, &["rev-parse", "--git-path", name], cancel)?)?;
    let file = match std::fs::File::open(root.join(path.strip_suffix('\n').unwrap_or(&path))) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Cannot inspect history boundaries: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read history boundaries: {error}"))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("History boundary metadata exceeds the 2 MiB limit".into());
    }
    check_cancel(cancel)?;
    Ok(bytes)
}

fn basis(root: &Path, all: bool, cancel: &AtomicBool) -> Result<Basis, String> {
    let head = git::text(run(root, &["rev-parse", "--revs-only", "HEAD"], cancel)?)?
        .trim()
        .to_owned();
    if !head.is_empty() && !valid_hash(&head) {
        return Err("Invalid history HEAD".into());
    }
    let mut args = vec![
        "for-each-ref",
        "--sort=refname",
        "--count=5001",
        "--format=%(refname)%00%(objectname)%00%(symref)%00",
    ];
    if !all {
        args.push("refs/replace/");
    }
    let refs = run(root, &args, cancel)?;
    if refs
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
        .count()
        > REF_LIMIT
    {
        return Err("History exceeds the 5,000-ref snapshot limit".into());
    }
    Ok(Basis {
        head,
        refs,
        shallow: boundary(root, "shallow", cancel)?,
        grafts: boundary(root, "info/grafts", cancel)?,
    })
}

fn verify(root: &Path, cursor: &Cursor, cancel: &AtomicBool) -> Result<(), String> {
    if basis(root, cursor.all_branches, cancel)? != cursor.basis {
        return Err("History changed (HEAD, refs or history boundaries). Reload before loading more; existing rows are retained.".into());
    }
    Ok(())
}

/// Capture a bounded topological ID order, then read commit data in bounded pages.
pub fn reload(
    root: &Path,
    all: bool,
    requested: usize,
    keep: &[String],
    cancel: &AtomicBool,
) -> Result<Page, String> {
    first_with_limits(root, all, requested, keep, PAGE_SIZE, HISTORY_LIMIT, cancel)
}

fn first_with_limits(
    root: &Path,
    all: bool,
    requested: usize,
    keep: &[String],
    page_size: usize,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Page, String> {
    if page_size == 0 || page_size > PAGE_SIZE || limit == 0 || limit > HISTORY_LIMIT {
        return Err("Invalid history page limits".into());
    }
    let initial = basis(root, all, cancel)?;
    let count = format!("--max-count={}", limit + 1);
    let mut args = vec!["rev-list", "--topo-order", &count];
    if all {
        args.push("--all");
    }
    if !initial.head.is_empty() {
        args.push(&initial.head);
    }
    args.push("--");
    let output = if initial.head.is_empty() && !all {
        String::new()
    } else {
        git::text(run(root, &args, cancel)?)?
    };
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for id in output.lines() {
        if !valid_hash(id) || !seen.insert(id.to_owned()) {
            return Err("Invalid or duplicate history snapshot ID".into());
        }
        ids.push(id.to_owned());
    }
    let capped = ids.len() > limit;
    ids.truncate(limit);
    let selected_end = keep
        .iter()
        .filter_map(|id| ids.iter().position(|hash| hash == id))
        .map(|index| index + 1)
        .max()
        .unwrap_or(0);
    let end = requested.max(page_size).max(selected_end).min(ids.len());
    let cursor = Cursor {
        basis: initial,
        all_branches: all,
        ids,
        next: 0,
        capped,
        page_size,
    };
    verify(root, &cursor, cancel)?;
    read_to(root, cursor, Vec::new(), end, cancel)
}

pub fn more(
    root: &Path,
    cursor: &Cursor,
    previous: Vec<Commit>,
    cancel: &AtomicBool,
) -> Result<Page, String> {
    if !cursor.has_more() {
        return Err("No additional history is available within the safety limit".into());
    }
    if previous.len() != cursor.next
        || !previous
            .iter()
            .zip(&cursor.ids)
            .all(|(commit, id)| commit.hash == *id)
    {
        return Err(
            "Loaded history no longer matches its snapshot. Reload before retrying.".into(),
        );
    }
    let end = (cursor.next + cursor.page_size).min(cursor.ids.len());
    read_to(root, cursor.clone(), previous, end, cancel)
}

fn read_to(
    root: &Path,
    mut cursor: Cursor,
    mut commits: Vec<Commit>,
    end: usize,
    cancel: &AtomicBool,
) -> Result<Page, String> {
    verify(root, &cursor, cancel)?;
    while cursor.next < end {
        check_cancel(cancel)?;
        let next = (cursor.next + cursor.page_size).min(end);
        let ids = &cursor.ids[cursor.next..next];
        let input = format!("{}\n", ids.join("\n"));
        let mut batch = parse(&git::run_with_input_controlled(
            root,
            &[
                "log",
                "--no-walk=unsorted",
                "--no-patch",
                "--no-decorate",
                "--no-notes",
                "--no-show-signature",
                "--encoding=UTF-8",
                "-z",
                "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%B",
                "--stdin",
            ],
            input.as_bytes(),
            cancel,
        )?)?;
        if batch.len() != ids.len() || !batch.iter().zip(ids).all(|(commit, id)| commit.hash == *id)
        {
            return Err("Git returned a different history page. Reload before retrying.".into());
        }
        commits.append(&mut batch);
        cursor.next = next;
    }
    verify(root, &cursor, cancel)?;
    check_cancel(cancel)?;
    let truncated = cursor.has_more() || cursor.capped;
    Ok(Page {
        history: prepare(commits, truncated),
        cursor,
    })
}

#[cfg(test)]
pub fn load(root: &Path, all: bool) -> Result<History, String> {
    reload(root, all, PAGE_SIZE, &[], &AtomicBool::new(false)).map(|page| page.history)
}

#[cfg(test)]
fn load_limited(root: &Path, all: bool, limit: usize) -> Result<History, String> {
    first_with_limits(
        root,
        all,
        limit,
        &[],
        limit,
        HISTORY_LIMIT,
        &AtomicBool::new(false),
    )
    .map(|page| page.history)
}

fn valid_hash(hash: &str) -> bool {
    matches!(hash.len(), 40 | 64) && hash.bytes().all(|c| c.is_ascii_hexdigit())
}

fn parse(bytes: &[u8]) -> Result<Vec<Commit>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = bytes
        .strip_suffix(&[0])
        .ok_or("Truncated Git history output")?;
    let fields: Vec<_> = bytes.split(|b| *b == 0).collect();
    if !fields.len().is_multiple_of(6) {
        return Err("Malformed Git history record".into());
    }
    let mut seen = HashSet::new();
    fields
        .as_chunks::<6>()
        .0
        .iter()
        .map(|record| {
            let values: Vec<_> = record
                .iter()
                .map(|field| {
                    std::str::from_utf8(field)
                        .map_err(|_| "Non-UTF-8 history output is not supported".to_owned())
                })
                .collect::<Result<_, _>>()?;
            let hash = values[0];
            let parents: Vec<String> = values[1].split_whitespace().map(str::to_owned).collect();
            if !valid_hash(hash) || parents.iter().any(|p| !valid_hash(p)) || !seen.insert(hash) {
                return Err("Invalid or duplicate commit ID in Git history".into());
            }
            Ok(Commit {
                hash: hash.into(),
                parents,
                author: values[2].into(),
                email: values[3].into(),
                date: values[4].into(),
                message: values[5].trim_end_matches('\n').into(),
                lane: 0,
            })
        })
        .collect()
}

pub fn prepare(mut commits: Vec<Commit>, truncated: bool) -> History {
    let mut lanes: Vec<String> = Vec::new();
    let mut max_lane = 0;
    for commit in &mut commits {
        let lane = lanes
            .iter()
            .position(|h| h == &commit.hash)
            .unwrap_or_else(|| vacant_lane(&mut lanes));
        commit.lane = lane;
        max_lane = max_lane.max(lane);
        lanes[lane].clear();
        for (index, parent) in commit.parents.iter().enumerate() {
            if lanes.contains(parent) {
                continue;
            }
            let next = if index == 0 && lanes[lane].is_empty() {
                lane
            } else {
                vacant_lane(&mut lanes)
            };
            lanes[next] = parent.clone();
        }
    }
    let positions: HashMap<_, _> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.as_str(), i))
        .collect();
    let mut edges = Vec::new();
    for (child, commit) in commits.iter().enumerate() {
        for hash in &commit.parents {
            if let Some(&parent) = positions.get(hash.as_str()) {
                edges.push(HistoryEdge {
                    child,
                    parent,
                    child_lane: commit.lane,
                    parent_lane: commits[parent].lane,
                });
            }
        }
    }
    History {
        commits,
        edges,
        max_lane,
        truncated,
    }
}

fn vacant_lane(lanes: &mut Vec<String>) -> usize {
    lanes.iter().position(String::is_empty).unwrap_or_else(|| {
        lanes.push(String::new());
        lanes.len() - 1
    })
}

pub fn patch(root: &Path, hash: &str, first_parent: Option<&str>) -> Result<String, String> {
    if !valid_hash(hash) || first_parent.is_some_and(|p| !valid_hash(p)) {
        return Err("Invalid commit ID".into());
    }
    let args = if let Some(parent) = first_parent {
        vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            parent,
            hash,
            "--",
        ]
    } else {
        vec![
            "show",
            "--format=",
            "--no-notes",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--root",
            hash,
            "--",
        ]
    };
    let output = git::text(git::run(root, &args)?)?;
    git::limit_diff(if output.is_empty() {
        "This commit has no text changes against its first parent.".into()
    } else {
        output
    })
}

#[cfg(test)]
mod tests;
