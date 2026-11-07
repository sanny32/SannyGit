#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "U",
            Self::Conflicted => "C",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Added => "Added",
            Self::Deleted => "Deleted",
            Self::Renamed => "Renamed",
            Self::Untracked => "Untracked",
            Self::Conflicted => "Conflicted",
        }
    }
}

pub struct Repository {
    pub name: String,
    pub path: String,
}

pub struct ChangeFile {
    pub path: String,
    pub original_path: Option<String>,
    pub kind: ChangeKind,
    pub staged: bool,
    pub unstaged: bool,
    pub diff: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

pub struct BranchRef {
    pub name: String,
    pub full_name: String,
    pub hash: String,
    pub upstream: Option<String>,
    pub current: bool,
    pub kind: RefKind,
}

pub struct RefSnapshot {
    pub refs: Vec<BranchRef>,
    pub has_head: bool,
}

pub struct Commit {
    pub message: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub hash: String,
    pub parents: Vec<String>,
    pub lane: usize,
}

impl Commit {
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("(no message)")
    }
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || [&self.message, &self.author, &self.email, &self.hash]
                .iter()
                .any(|value| value.to_lowercase().contains(query))
    }
}

pub struct HistoryEdge {
    pub child: usize,
    pub parent: usize,
    pub child_lane: usize,
    pub parent_lane: usize,
}

pub struct History {
    pub commits: Vec<Commit>,
    pub edges: Vec<HistoryEdge>,
    pub max_lane: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Header,
    Hunk,
    Context,
    Added,
    Removed,
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

/// Parsed once on the worker, then rendered using only visible rows.
pub struct DiffPreview {
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
    pub max_columns: usize,
}

impl DiffPreview {
    pub fn new(patch: &str) -> Self {
        let lines = crate::diff::parse_unified_diff(patch);
        let added = lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Added)
            .count();
        let removed = lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Removed)
            .count();
        let max_columns = lines
            .iter()
            .map(|l| {
                l.text
                    .chars()
                    .map(|c| {
                        if c == '\t' {
                            4
                        } else if c.is_ascii() {
                            1
                        } else {
                            2
                        }
                    })
                    .sum()
            })
            .max()
            .unwrap_or(0);
        Self {
            lines,
            added,
            removed,
            max_columns,
        }
    }
}
