//! Partial patches for staging, unstaging, and discarding hunks or lines.
//!
//! Stage applies a forward patch to the index. Unselected deletions stay in the
//! index as context, and unselected additions are left out. Unstage and discard
//! build the reverse patch, omitting every unselected change so the patch matches
//! the side Git will edit.

use crate::models::{DiffLine, DiffLineKind};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffEdit {
    Stage,
    Unstage,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    None,
    Staged,
    Worktree,
    Untracked,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchPlan {
    Apply(String),
    /// Drop selected added lines from an untracked file. An empty keep list deletes it.
    RewriteUntracked {
        path: String,
        kept_lines: Vec<String>,
    },
}

pub fn partial_patch(
    lines: &[DiffLine],
    selected: &BTreeSet<usize>,
    edit: DiffEdit,
) -> Result<PatchPlan, String> {
    if selected.is_empty() {
        return Err("Select one or more changed lines.".into());
    }
    let areas = areas(lines);
    let mut area = None;
    for &index in selected {
        let line = lines
            .get(index)
            .ok_or("A selected line is no longer in this diff.")?;
        if !matches!(line.kind, DiffLineKind::Added | DiffLineKind::Removed) {
            return Err("Select changed lines. Context and headers stay in place.".into());
        }
        let line_area = areas[index];
        if line_area == Area::Conflict {
            return Err("Resolve this file in Conflict Solver before editing lines.".into());
        }
        if line_area == Area::None {
            return Err("This line is not part of a stageable change.".into());
        }
        match area {
            None => area = Some(line_area),
            Some(current) if current != line_area => {
                return Err(
                    "Select lines from staged changes or from working-tree changes, not both."
                        .into(),
                );
            }
            _ => {}
        }
    }
    let area = area.unwrap_or(Area::None);
    match (edit, area) {
        (DiffEdit::Stage, Area::Worktree) => {
            Ok(PatchPlan::Apply(git_patch(lines, selected, area, false)?))
        }
        (DiffEdit::Stage, Area::Untracked) => {
            Ok(PatchPlan::Apply(untracked_stage(lines, selected)?))
        }
        (DiffEdit::Unstage, Area::Staged) => {
            Ok(PatchPlan::Apply(git_patch(lines, selected, area, true)?))
        }
        (DiffEdit::Discard, Area::Worktree) => {
            Ok(PatchPlan::Apply(git_patch(lines, selected, area, true)?))
        }
        (DiffEdit::Discard, Area::Untracked) => untracked_rewrite(lines, selected),
        (DiffEdit::Stage, Area::Staged) => {
            Err("These lines are already staged. Unstage them, or stage working-tree lines.".into())
        }
        (DiffEdit::Unstage, _) => Err("Select staged lines to unstage.".into()),
        _ => Err("Those lines cannot be changed that way.".into()),
    }
}

/// Changed line indexes belonging to the hunk header at `hunk`.
pub fn hunk_changes(lines: &[DiffLine], hunk: usize) -> BTreeSet<usize> {
    let mut selected = BTreeSet::new();
    if lines
        .get(hunk)
        .is_none_or(|line| line.kind != DiffLineKind::Hunk)
    {
        return selected;
    }
    for (index, line) in lines.iter().enumerate().skip(hunk + 1) {
        if line.kind == DiffLineKind::Hunk
            || is_banner(&line.text).is_some()
            || line.text.starts_with("diff --git ")
        {
            break;
        }
        if matches!(line.kind, DiffLineKind::Added | DiffLineKind::Removed) {
            selected.insert(index);
        }
    }
    selected
}

pub fn edit_summary(path: &str, count: usize, edit: DiffEdit) -> String {
    let verb = match edit {
        DiffEdit::Stage => "Stage",
        DiffEdit::Unstage => "Unstage",
        DiffEdit::Discard => "Discard",
    };
    if count == 1 {
        format!("{verb} 1 selected line in {path}?")
    } else {
        format!("{verb} {count} selected lines in {path}?")
    }
}

fn areas(lines: &[DiffLine]) -> Vec<Area> {
    let mut area = Area::None;
    lines
        .iter()
        .map(|line| {
            if let Some(next) = is_banner(&line.text) {
                area = next;
            }
            area
        })
        .collect()
}

fn is_banner(text: &str) -> Option<Area> {
    if text.starts_with("Staged changes") {
        Some(Area::Staged)
    } else if text.starts_with("Working tree changes") {
        Some(Area::Worktree)
    } else if text.starts_with("Conflict:") {
        Some(Area::Conflict)
    } else if text == "Untracked file" {
        Some(Area::Untracked)
    } else {
        None
    }
}

fn git_patch(
    lines: &[DiffLine],
    selected: &BTreeSet<usize>,
    area: Area,
    reverse: bool,
) -> Result<String, String> {
    let mut output = String::new();
    let mut current = Area::None;
    let mut header: Vec<&str> = Vec::new();
    let mut header_written = false;
    let mut emitted = false;
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        if let Some(next) = is_banner(&line.text) {
            current = next;
            header.clear();
            header_written = false;
            index += 1;
            continue;
        }
        if line.text.starts_with("diff --git ") {
            header.clear();
            header.push(&line.text);
            header_written = false;
            index += 1;
            continue;
        }
        if current == area && line.kind == DiffLineKind::Hunk {
            let body = emit_hunk(lines, &mut index, selected, reverse)?;
            if body.is_empty() {
                continue;
            }
            if !header_written {
                if header.iter().any(|line| {
                    line.contains("Binary files")
                        || line.contains("GIT binary patch")
                        || line.starts_with("rename from ")
                        || line.starts_with("rename to ")
                        || line.starts_with("copy from ")
                }) {
                    return Err(
                        "Stage, unstage, or discard this file as a whole. Line edits do not apply to binary or renamed files.".into(),
                    );
                }
                for line in &header {
                    output.push_str(line);
                    output.push('\n');
                }
                header_written = true;
            }
            output.push_str(&body);
            emitted = true;
            continue;
        }
        if current == area
            && line.kind == DiffLineKind::Header
            && !line.text.starts_with("\\ No newline")
        {
            header.push(&line.text);
        }
        index += 1;
    }
    if !emitted {
        return Err("The selection does not contain a change that can be applied.".into());
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn emit_hunk(
    lines: &[DiffLine],
    index: &mut usize,
    selected: &BTreeSet<usize>,
    reverse: bool,
) -> Result<String, String> {
    let header = &lines[*index].text;
    let (old, new) = hunk_ranges(header).ok_or("Could not read a hunk header.")?;
    let suffix = hunk_suffix(header);
    let mut old_cursor = cursor(old.0, old.1);
    let mut new_cursor = cursor(new.0, new.1);
    let mut output = String::new();
    let mut group: Option<Group> = None;
    *index += 1;
    while *index < lines.len() {
        let line = &lines[*index];
        if line.kind == DiffLineKind::Hunk
            || is_banner(&line.text).is_some()
            || line.text.starts_with("diff --git ")
        {
            break;
        }
        if line.kind == DiffLineKind::Header && line.text.starts_with("\\ No newline") {
            if let Some(group) = &mut group {
                if group.attach_newline {
                    group.body.push_str(&line.text);
                    group.body.push('\n');
                }
            }
            *index += 1;
            continue;
        }
        let action = classify(line, selected.contains(index), reverse);
        if matches!(action, Action::Skip) {
            // Omitting an addition leaves the old side contiguous. Omitting a
            // deletion, which reverse edits do, has to start a new hunk.
            if reverse {
                flush(&mut group, suffix, &mut output);
            }
            if let Some(group) = &mut group {
                group.attach_newline = false;
            }
            advance(line, &mut old_cursor, &mut new_cursor);
            *index += 1;
            continue;
        }
        let Action::Emit {
            prefix,
            counts_old,
            counts_new,
            selected: chosen,
        } = action
        else {
            unreachable!("skip is handled above");
        };
        let group = group.get_or_insert_with(|| Group {
            old_cursor,
            new_cursor,
            old_count: 0,
            new_count: 0,
            has_selected: false,
            attach_newline: false,
            body: String::new(),
        });
        group.has_selected |= chosen;
        if counts_old {
            group.old_count += 1;
        }
        if counts_new {
            group.new_count += 1;
        }
        group.body.push(prefix);
        group.body.push_str(&line.text);
        group.body.push('\n');
        group.attach_newline = true;
        advance(line, &mut old_cursor, &mut new_cursor);
        *index += 1;
    }
    flush(&mut group, suffix, &mut output);
    Ok(output)
}

struct Group {
    old_cursor: u32,
    new_cursor: u32,
    old_count: u32,
    new_count: u32,
    has_selected: bool,
    attach_newline: bool,
    body: String,
}

fn flush(group: &mut Option<Group>, suffix: &str, output: &mut String) {
    let Some(group) = group.take() else {
        return;
    };
    if !group.has_selected {
        return;
    }
    let old_start = if group.old_count == 0 {
        group.old_cursor.saturating_sub(1)
    } else {
        group.old_cursor
    };
    let new_start = if group.new_count == 0 {
        group.new_cursor.saturating_sub(1)
    } else {
        group.new_cursor
    };
    output.push_str(&format!(
        "@@ -{old_start},{count_old} +{new_start},{count_new} @@{suffix}\n{body}",
        count_old = group.old_count,
        count_new = group.new_count,
        body = group.body
    ));
}

enum Action {
    Skip,
    Emit {
        prefix: char,
        counts_old: bool,
        counts_new: bool,
        selected: bool,
    },
}

fn classify(line: &DiffLine, selected: bool, reverse: bool) -> Action {
    match line.kind {
        DiffLineKind::Context => Action::Emit {
            prefix: ' ',
            counts_old: true,
            counts_new: true,
            selected: false,
        },
        DiffLineKind::Added if selected => Action::Emit {
            prefix: '+',
            counts_old: false,
            counts_new: true,
            selected: true,
        },
        DiffLineKind::Removed if selected => Action::Emit {
            prefix: '-',
            counts_old: true,
            counts_new: false,
            selected: true,
        },
        // Keep an unselected deletion in the index by turning it into context.
        DiffLineKind::Removed if !reverse => Action::Emit {
            prefix: ' ',
            counts_old: true,
            counts_new: true,
            selected: false,
        },
        DiffLineKind::Added | DiffLineKind::Removed => Action::Skip,
        DiffLineKind::Header | DiffLineKind::Hunk => Action::Skip,
    }
}

fn advance(line: &DiffLine, old_cursor: &mut u32, new_cursor: &mut u32) {
    match line.kind {
        DiffLineKind::Context => {
            *old_cursor += 1;
            *new_cursor += 1;
        }
        DiffLineKind::Removed => *old_cursor += 1,
        DiffLineKind::Added => *new_cursor += 1,
        DiffLineKind::Header | DiffLineKind::Hunk => {}
    }
}

fn untracked_stage(lines: &[DiffLine], selected: &BTreeSet<usize>) -> Result<String, String> {
    let path = untracked_path(lines)?;
    let added = untracked_added(lines);
    if added.is_empty() {
        return Err("This untracked file has no text lines to stage.".into());
    }
    if !selected
        .iter()
        .all(|index| added.iter().any(|(i, _)| i == index))
    {
        return Err("Select lines from the untracked file.".into());
    }
    let kept: Vec<_> = added
        .iter()
        .filter(|(index, _)| selected.contains(index))
        .map(|(_, text)| *text)
        .collect();
    if kept.is_empty() {
        return Err("Select one or more lines to stage.".into());
    }
    let left = quote_git_path(&format!("a/{path}"));
    let right = quote_git_path(&format!("b/{path}"));
    let mut patch = format!(
        "diff --git {left} {right}\nnew file mode 100644\n--- /dev/null\n+++ {right}\n@@ -0,0 +1,{} @@\n",
        kept.len()
    );
    for line in kept {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    Ok(patch)
}

fn untracked_rewrite(lines: &[DiffLine], selected: &BTreeSet<usize>) -> Result<PatchPlan, String> {
    let path = untracked_path(lines)?;
    let added = untracked_added(lines);
    if !selected
        .iter()
        .all(|index| added.iter().any(|(i, _)| i == index))
    {
        return Err("Select lines from the untracked file.".into());
    }
    if !selected
        .iter()
        .any(|index| added.iter().any(|(i, _)| i == index))
    {
        return Err("Select one or more lines to discard.".into());
    }
    Ok(PatchPlan::RewriteUntracked {
        path,
        kept_lines: added
            .into_iter()
            .filter(|(index, _)| !selected.contains(index))
            .map(|(_, text)| text.to_owned())
            .collect(),
    })
}

fn untracked_path(lines: &[DiffLine]) -> Result<String, String> {
    lines
        .iter()
        .find_map(|line| line.text.strip_prefix("+++ ").map(str::trim))
        .filter(|path| !path.is_empty() && *path != "/dev/null")
        .map(str::to_owned)
        .ok_or_else(|| "Could not identify the untracked file.".into())
}

fn untracked_added(lines: &[DiffLine]) -> Vec<(usize, &str)> {
    let mut area = Area::None;
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            if let Some(next) = is_banner(&line.text) {
                area = next;
            }
            (area == Area::Untracked && line.kind == DiffLineKind::Added)
                .then_some((index, line.text.as_str()))
        })
        .collect()
}

fn quote_git_path(path: &str) -> String {
    if path
        .bytes()
        .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'@' | b'%' | b'+' | b'=' | b',' | b'~' | b'/' | b'-'))
    {
        path.to_owned()
    } else {
        format!(
            "\"{}\"",
            path.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }
}

fn hunk_ranges(header: &str) -> Option<((u32, u32), (u32, u32))> {
    let mut old = None;
    let mut new = None;
    for token in header.split_whitespace() {
        if let Some(rest) = token.strip_prefix('-') {
            if !rest.starts_with('-') {
                old = parse_range(rest);
            }
        } else if let Some(rest) = token.strip_prefix('+') {
            new = parse_range(rest);
        }
    }
    Some((old?, new?))
}

fn parse_range(token: &str) -> Option<(u32, u32)> {
    let mut parts = token.split(',');
    let start = parts.next()?.parse().ok()?;
    let count = match parts.next() {
        Some(value) => value.parse().ok()?,
        None => 1,
    };
    Some((start, count))
}

fn cursor(start: u32, count: u32) -> u32 {
    if count == 0 {
        start + 1
    } else {
        start
    }
}

fn hunk_suffix(header: &str) -> &str {
    let mut rest = header;
    for _ in 0..2 {
        if let Some(position) = rest.find("@@") {
            rest = &rest[position + 2..];
        }
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse_unified_diff;

    fn lines(patch: &str) -> Vec<DiffLine> {
        parse_unified_diff(patch)
    }

    fn indexes(lines: &[DiffLine], text: &str) -> BTreeSet<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.text == text)
            .map(|(index, _)| index)
            .collect()
    }

    const REPLACEMENT: &str = "\
Working tree changes (index → working tree)
diff --git a/file b/file
index 1111111..2222222 100644
--- a/file
+++ b/file
@@ -1,3 +1,3 @@ context
 one
-two
+TWO
 three
";

    #[test]
    fn staging_one_side_of_a_replacement_keeps_the_other_line() {
        let lines = lines(REPLACEMENT);
        let addition = indexes(&lines, "TWO");
        let plan = partial_patch(&lines, &addition, DiffEdit::Stage).unwrap();
        let PatchPlan::Apply(patch) = plan else {
            panic!("expected a patch");
        };
        assert!(patch.contains("@@ -1,3 +1,4 @@ context\n"));
        assert!(patch.contains(" one\n two\n+TWO\n three\n"), "{patch}");
        assert!(!patch.contains("-two"));

        let removal = indexes(&lines, "two");
        let PatchPlan::Apply(patch) = partial_patch(&lines, &removal, DiffEdit::Stage).unwrap()
        else {
            panic!("expected a patch");
        };
        assert!(patch.contains("@@ -1,3 +1,2 @@ context\n"));
        assert!(patch.contains(" one\n-two\n three\n"), "{patch}");
        assert!(!patch.contains("+TWO"));
    }

    #[test]
    fn discarding_an_addition_omits_the_unstaged_deletion() {
        let lines = lines(REPLACEMENT);
        let PatchPlan::Apply(patch) =
            partial_patch(&lines, &indexes(&lines, "TWO"), DiffEdit::Discard).unwrap()
        else {
            panic!("expected a patch");
        };
        assert!(patch.contains("+TWO\n"), "{patch}");
        assert!(!patch.contains("-two\n"), "{patch}");
        assert!(patch.contains(" three\n"), "{patch}");
    }

    #[test]
    fn a_second_hunk_can_be_staged_without_the_first() {
        let lines = lines(
            "\
Working tree changes (index → working tree)
diff --git a/file b/file
--- a/file
+++ b/file
@@ -1,3 +1,3 @@
 one
-two
+TWO
 three
@@ -10,3 +10,3 @@
 ten
-eleven
+ELEVEN
 twelve
",
        );
        let mut selected = indexes(&lines, "eleven");
        selected.extend(indexes(&lines, "ELEVEN"));
        let PatchPlan::Apply(patch) = partial_patch(&lines, &selected, DiffEdit::Stage).unwrap()
        else {
            panic!("expected a patch");
        };
        assert!(!patch.contains("TWO"), "{patch}");
        assert!(patch.contains("-eleven\n+ELEVEN\n"), "{patch}");
        assert!(patch.contains("@@ -10,3 +10,3 @@\n"), "{patch}");
    }

    #[test]
    fn untracked_stage_keeps_only_the_selected_lines() {
        let lines = lines(
            "\
Untracked file
--- /dev/null
+++ notes/new file.txt
@@ -0,0 +1,3 @@
+one
+two
+three
",
        );
        let PatchPlan::Apply(patch) =
            partial_patch(&lines, &indexes(&lines, "two"), DiffEdit::Stage).unwrap()
        else {
            panic!("expected a patch");
        };
        assert!(patch.contains("diff --git \"a/notes/new file.txt\" \"b/notes/new file.txt\"\n"));
        assert!(patch.contains("@@ -0,0 +1,1 @@\n+two\n"), "{patch}");
        let PatchPlan::RewriteUntracked { path, kept_lines } =
            partial_patch(&lines, &indexes(&lines, "two"), DiffEdit::Discard).unwrap()
        else {
            panic!("expected a rewrite");
        };
        assert_eq!(path, "notes/new file.txt");
        assert_eq!(kept_lines, vec!["one".to_owned(), "three".to_owned()]);
    }
}
