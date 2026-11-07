//! Bounded history reads and graph preparation, executed by background workers.
use crate::{
    git,
    models::{Commit, History, HistoryEdge},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub const HISTORY_LIMIT: usize = 500;

pub fn load(root: &Path, all_branches: bool) -> Result<History, String> {
    load_limited(root, all_branches, HISTORY_LIMIT)
}

fn load_limited(root: &Path, all_branches: bool, limit: usize) -> Result<History, String> {
    let head = git::run(root, &["rev-parse", "--revs-only", "HEAD"])?;
    if head.is_empty() && !all_branches {
        return Ok(prepare(Vec::new(), false));
    }
    let count = format!("--max-count={}", limit + 1);
    let mut args = vec![
        "log",
        "--topo-order",
        "--no-patch",
        "--no-decorate",
        "--no-notes",
        "--no-show-signature",
        "--encoding=UTF-8",
        "-z",
        "--format=tformat:%H%x00%P%x00%an%x00%ae%x00%aI%x00%B",
        &count,
    ];
    if all_branches {
        args.push("--all");
    }
    if !head.is_empty() {
        args.push("HEAD");
    }
    args.push("--");
    let mut commits = parse(&git::run(root, &args)?)?;
    let truncated = commits.len() > limit;
    commits.truncate(limit);
    Ok(prepare(commits, truncated))
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
mod tests {
    use super::*;
    use crate::git::tests::TempRepo;

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
