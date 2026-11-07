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

pub struct Commit {
    pub message: &'static str,
    pub author: &'static str,
    pub date: &'static str,
    pub hash: &'static str,
    pub parents: &'static [&'static str],
    pub lane: usize,
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
