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
pub(crate) mod tests {
    use super::*;
    use crate::git::tests::TempRepo;

    pub(crate) fn linear_repo(count: usize) -> TempRepo {
        let repo = TempRepo::new();
        let mut input = String::new();
        for index in 1..=count {
            let message = format!("History commit {index}\n");
            let contents = format!("Contents {index}\n");
            input.push_str(&format!("commit refs/heads/main\nmark :{index}\ncommitter History Test <history@example.invalid> {} +0000\ndata {}\n{message}", 1_700_000_000 + index, message.len()));
            if index > 1 {
                input.push_str(&format!("from :{}\n", index - 1));
            }
            input.push_str(&format!(
                "M 100644 inline file\ndata {}\n{contents}\n",
                contents.len()
            ));
        }
        git::run_with_input(&repo.0, &["fast-import", "--quiet"], Some(input.as_bytes())).unwrap();
        repo.write("untracked", "keep local bytes\n");
        repo
    }

    fn first(root: &Path, all: bool, limit: usize) -> Page {
        first_with_limits(root, all, 2, &[], 2, limit, &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn real_pages_cross_the_old_limit_without_duplicate_commits_or_index_writes() {
        let repo = linear_repo(620);
        let head = repo.git(&["rev-parse", "HEAD"]);
        let index = std::fs::read(repo.0.join(".git/index")).ok();
        let cancel = AtomicBool::new(false);
        let first = reload(&repo.0, false, PAGE_SIZE, &[], &cancel).unwrap();
        assert_eq!(first.history.commits.len(), PAGE_SIZE);
        assert!(first.cursor.has_more());
        let prefix = first
            .history
            .commits
            .iter()
            .map(|commit| (commit.hash.clone(), commit.lane))
            .collect::<Vec<_>>();
        assert!(more(
            &repo.0,
            &first.cursor,
            first.history.commits.clone(),
            &AtomicBool::new(true)
        )
        .is_err());
        let complete = more(&repo.0, &first.cursor, first.history.commits, &cancel).unwrap();
        assert_eq!(complete.history.commits.len(), 620);
        assert!(!complete.cursor.has_more());
        assert!(!complete.history.truncated);
        assert_eq!(
            complete
                .history
                .commits
                .iter()
                .take(PAGE_SIZE)
                .map(|commit| (commit.hash.clone(), commit.lane))
                .collect::<Vec<_>>(),
            prefix
        );
        let expected = repo.git(&["rev-list", "--topo-order", "HEAD"]);
        assert_eq!(
            complete
                .history
                .commits
                .iter()
                .map(|commit| commit.hash.as_str())
                .collect::<Vec<_>>(),
            expected.lines().collect::<Vec<_>>()
        );
        assert!(complete
            .history
            .edges
            .iter()
            .any(|edge| edge.child == PAGE_SIZE - 1 && edge.parent == PAGE_SIZE));
        assert_eq!(std::fs::read(repo.0.join(".git/index")).ok(), index);
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(
            std::fs::read(repo.0.join("untracked")).unwrap(),
            b"keep local bytes\n"
        );
    }

    #[test]
    fn changed_head_requires_reload_and_reload_keeps_requested_old_commit() {
        let repo = linear_repo(6);
        let page = first(&repo.0, false, HISTORY_LIMIT);
        let selected = page.history.commits[1].hash.clone();
        repo.git(&["commit", "--allow-empty", "-m", "New HEAD"]);
        assert!(more(
            &repo.0,
            &page.cursor,
            page.history.commits.clone(),
            &AtomicBool::new(false)
        )
        .err()
        .unwrap()
        .contains("History changed"));
        let refreshed = first_with_limits(
            &repo.0,
            false,
            2,
            std::slice::from_ref(&selected),
            2,
            HISTORY_LIMIT,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(refreshed.history.commits.len(), 3);
        assert_eq!(refreshed.history.commits[2].hash, selected);
        assert_eq!(page.cursor.loaded(), 2);
    }

    #[test]
    fn all_refs_changes_and_shallow_boundaries_cannot_mix_history_pages() {
        let repo = linear_repo(6);
        let all = first(&repo.0, true, HISTORY_LIMIT);
        let current = first(&repo.0, false, HISTORY_LIMIT);
        repo.git(&["tag", "new-ref"]);
        assert!(more(
            &repo.0,
            &all.cursor,
            all.history.commits,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(more(
            &repo.0,
            &current.cursor,
            current.history.commits.clone(),
            &AtomicBool::new(false)
        )
        .is_ok());
        let cutoff = repo.git(&["rev-parse", "HEAD~2"]);
        std::fs::write(repo.0.join(".git/shallow"), &cutoff).unwrap();
        assert!(more(
            &repo.0,
            &current.cursor,
            current.history.commits,
            &AtomicBool::new(false)
        )
        .is_err());
        let shallow = first(&repo.0, false, HISTORY_LIMIT);
        let complete = more(
            &repo.0,
            &shallow.cursor,
            shallow.history.commits,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(complete.history.commits.len(), 3);
        assert!(complete.history.commits[2].parents.is_empty());
    }

    #[test]
    fn snapshot_limits_empty_repositories_and_mismatched_prefixes_are_explicit() {
        let empty = TempRepo::new();
        assert!(
            !reload(&empty.0, true, PAGE_SIZE, &[], &AtomicBool::new(false))
                .unwrap()
                .cursor
                .has_more()
        );
        let repo = linear_repo(6);
        let capped = first(&repo.0, false, 3);
        assert!(more(&repo.0, &capped.cursor, Vec::new(), &AtomicBool::new(false)).is_err());
        let last = more(
            &repo.0,
            &capped.cursor,
            capped.history.commits,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(last.history.commits.len(), 3);
        assert!(last.cursor.at_limit() && last.history.truncated);
        assert!(!last.cursor.has_more());
        assert!(more(
            &repo.0,
            &last.cursor,
            last.history.commits,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(first_with_limits(
            &repo.0,
            false,
            1,
            &[],
            0,
            HISTORY_LIMIT,
            &AtomicBool::new(false)
        )
        .is_err());
    }

    #[test]
    fn paged_merge_graph_preserves_lanes_and_links_parents_across_batches() {
        let repo = TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "Root"]);
        repo.git(&["switch", "-c", "topic"]);
        repo.git(&["commit", "--allow-empty", "-m", "Topic"]);
        repo.git(&["switch", "main"]);
        repo.git(&["commit", "--allow-empty", "-m", "Main"]);
        repo.git(&["merge", "--no-ff", "topic", "-m", "Merge"]);
        let mut page = first(&repo.0, false, HISTORY_LIMIT);
        let lanes = page
            .history
            .commits
            .iter()
            .map(|commit| (commit.hash.clone(), commit.lane))
            .collect::<Vec<_>>();
        while page.cursor.has_more() {
            page = more(
                &repo.0,
                &page.cursor,
                page.history.commits,
                &AtomicBool::new(false),
            )
            .unwrap();
        }
        assert_eq!(
            page.history
                .commits
                .iter()
                .take(2)
                .map(|commit| (commit.hash.clone(), commit.lane))
                .collect::<Vec<_>>(),
            lanes
        );
        assert_eq!(
            page.history
                .edges
                .iter()
                .filter(|edge| edge.child == 0)
                .count(),
            2
        );
        for edge in &page.history.edges {
            assert!(edge.child < edge.parent);
            assert!(page.history.commits[edge.child]
                .parents
                .contains(&page.history.commits[edge.parent].hash));
        }
    }

    #[test]
    fn oversized_later_page_fails_without_changing_the_previous_page_or_cursor() {
        let repo = TempRepo::new();
        let tree =
            git::text(git::run_with_input(&repo.0, &["mktree"], Some(b"")).unwrap()).unwrap();
        let huge = "Huge message ".repeat(200_000);
        let root = git::text(
            git::run_with_input(
                &repo.0,
                &["commit-tree", tree.trim()],
                Some(huge.as_bytes()),
            )
            .unwrap(),
        )
        .unwrap();
        let mut parent = root.trim().to_owned();
        for message in ["Second", "Third"] {
            parent = git::text(
                git::run_with_input(
                    &repo.0,
                    &["commit-tree", tree.trim(), "-p", &parent],
                    Some(message.as_bytes()),
                )
                .unwrap(),
            )
            .unwrap()
            .trim()
            .to_owned();
        }
        repo.git(&["update-ref", "refs/heads/main", &parent]);
        let page = first(&repo.0, false, HISTORY_LIMIT);
        let hashes = page
            .history
            .commits
            .iter()
            .map(|commit| commit.hash.clone())
            .collect::<Vec<_>>();
        let error = more(
            &repo.0,
            &page.cursor,
            page.history.commits.clone(),
            &AtomicBool::new(false),
        )
        .err()
        .unwrap();
        assert!(error.contains("limit"));
        assert_eq!(page.cursor.loaded(), 2);
        assert_eq!(
            page.history
                .commits
                .iter()
                .map(|commit| commit.hash.clone())
                .collect::<Vec<_>>(),
            hashes
        );
    }

    #[cfg(unix)]
    #[test]
    fn head_change_during_message_read_is_detected_before_the_page_is_published() {
        std::thread::spawn(|| {
            use std::os::unix::fs::PermissionsExt;
            let repo = linear_repo(6);
            let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
            let next = git::text(git::run_with_input(&repo.0, &["commit-tree", tree.trim(), "-p", "HEAD"], Some(b"New head")).unwrap()).unwrap();
            let wrapper = repo.0.join(".git/changing-history-git");
            let script = format!("#!/bin/sh\nchange=0\nfor arg in \"$@\"; do if [ \"$arg\" = log ]; then change=1; fi; done\ngit \"$@\"\nresult=$?\nif [ \"$change\" = 1 ]; then git update-ref HEAD {}; fi\nexit \"$result\"\n", next.trim());
            std::fs::write(&wrapper, script).unwrap(); std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
            git::set_executable(wrapper.to_string_lossy().into_owned());
            let error = first_with_limits(&repo.0, false, 2, &[], 2, HISTORY_LIMIT, &AtomicBool::new(false)).err().unwrap();
            assert!(error.contains("History changed"));
        }).join().unwrap();
    }

    #[test]
    fn parses_full_messages_unicode_merge_parents_and_sha256_ids() {
        let hash = "a".repeat(40);
        let parent = "b".repeat(40);
        let other = "c".repeat(40);
        let bytes = format!("{hash}\0{parent} {other}\0Author é\0mail@example.invalid\02026-11-07T12:30:00+07:00\0Subject é\n\nBody\twith separators \u{1e} and \u{1f}\n\0");
        let commits = parse(bytes.as_bytes()).unwrap();
        assert_eq!(commits[0].parents, vec![parent, other]);
        assert_eq!(commits[0].subject(), "Subject é");
        assert!(commits[0].matches("body"));
        assert!(commits[0].matches("author é"));
        assert!(commits[0].matches("mail@"));
        assert!(commits[0].matches("aaaa"));
        let sha256 = format!("{}\0\0Author\0email\0date\0Root\0", "a".repeat(64));
        assert_eq!(parse(sha256.as_bytes()).unwrap()[0].hash.len(), 64);
    }

    #[test]
    fn rejects_truncated_malformed_non_utf8_and_duplicate_records() {
        let record = format!("{}\0\0Author\0email\0date\0Message\0", "a".repeat(40));
        assert!(parse(&record.as_bytes()[..record.len() - 1]).is_err());
        assert!(parse(b"short\0fields\0").is_err());
        assert!(parse(format!("{record}{record}").as_bytes()).is_err());
        assert!(parse(record.replace(&"a".repeat(40), "--help").as_bytes()).is_err());
        let mut invalid = record.into_bytes();
        invalid[42] = 255;
        assert!(parse(&invalid).is_err());
    }

    #[test]
    fn loads_real_scopes_graph_messages_and_first_parent_patches_without_writes() {
        let repo = TempRepo::new();
        assert!(load(&repo.0, false).unwrap().commits.is_empty());
        assert!(load(&repo.0, true).unwrap().commits.is_empty());
        repo.write("root", "root\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "Root"]);
        let root = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["checkout", "-b", "topic"]);
        repo.write("topic", "topic\n");
        repo.git(&["add", "."]);
        repo.git(&[
            "commit",
            "-m",
            "Topic é",
            "-m",
            "Full body\nwith another line",
        ]);
        let topic = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["checkout", "main"]);
        repo.write("main", "main\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "Main"]);
        assert_eq!(load(&repo.0, false).unwrap().commits.len(), 2);
        let all = load(&repo.0, true).unwrap();
        assert_eq!(all.commits.len(), 3);
        assert!(all.commits.iter().any(|c| c.message.contains("Full body")));
        repo.git(&["merge", "--no-ff", "topic", "-m", "Merge topic"]);
        repo.write("root", "unsaved worktree change\n");
        let index = std::fs::read(repo.0.join(".git/index")).unwrap();
        let history = load(&repo.0, false).unwrap();
        assert_eq!(history.commits.len(), 4);
        let merge = &history.commits[0];
        assert_eq!(merge.parents.len(), 2);
        assert_eq!(
            history.edges.iter().filter(|edge| edge.child == 0).count(),
            2
        );
        assert!(history.max_lane >= 1);
        for edge in &history.edges {
            assert!(edge.child < edge.parent);
            assert!(history.commits[edge.child]
                .parents
                .contains(&history.commits[edge.parent].hash));
        }
        let patch = patch(
            &repo.0,
            &merge.hash,
            merge.parents.first().map(String::as_str),
        )
        .unwrap();
        assert!(patch.contains("+topic"));
        assert!(!patch.contains("+main"));
        assert!(super::patch(&repo.0, &root, None)
            .unwrap()
            .contains("+root"));
        assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(repo.0.join("root")).unwrap(),
            "unsaved worktree change\n"
        );
        let limited = load_limited(&repo.0, false, 2).unwrap();
        assert!(limited.truncated);
        assert_eq!(limited.commits.len(), 2);
        assert!(limited.edges.iter().all(|e| e.parent < 2));
        repo.git(&["checkout", "--detach", &topic]);
        assert_eq!(load(&repo.0, false).unwrap().commits.len(), 2);
        assert_eq!(load(&repo.0, true).unwrap().commits.len(), 4);
    }

    #[test]
    fn handles_empty_patch_unborn_head_with_other_refs_and_invalid_objects() {
        let repo = TempRepo::new();
        repo.git(&["commit", "--allow-empty", "-m", "Empty"]);
        let history = load(&repo.0, false).unwrap();
        assert!(patch(&repo.0, &history.commits[0].hash, None)
            .unwrap()
            .contains("no text changes"));
        assert!(patch(&repo.0, "--help", None).is_err());
        assert!(patch(&repo.0, &"f".repeat(40), None).is_err());
        repo.git(&["checkout", "--orphan", "new-branch"]);
        assert!(load(&repo.0, false).unwrap().commits.is_empty());
        assert_eq!(load(&repo.0, true).unwrap().commits.len(), 1);
    }
}
