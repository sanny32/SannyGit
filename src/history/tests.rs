use super::*;
use crate::test_support::{linear_repo, TempRepo};

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
    let tree = git::text(git::run_with_input(&repo.0, &["mktree"], Some(b"")).unwrap()).unwrap();
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
