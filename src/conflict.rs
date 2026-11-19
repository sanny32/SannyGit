use crate::{discard, git};
use std::{
    fs,
    io::Write,
    ops::Range,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Edit,
    Ours,
    Theirs,
    Recreate,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "Conflict Solver",
            Self::Ours => "Take Ours",
            Self::Theirs => "Take Theirs",
            Self::Recreate => "Recreate Conflict",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Edit => "Review and resolve the selected file's conflict blocks.",
            Self::Ours => "Replace the working copy with Git's ours version and stage the result.",
            Self::Theirs => "Replace the working copy with Git's theirs version and stage the result.",
            Self::Recreate => "Replace the working copy with Git's conflict markers and restore unmerged index entries.",
        }
    }
}

#[derive(Clone)]
pub struct Conflict {
    pub span: Range<usize>,
    pub ours: Range<usize>,
    pub theirs: Range<usize>,
}

pub struct Review {
    index: Vec<u8>,
    undo: Vec<u8>,
    file: discard::FileState,
    pub stages: [bool; 4],
    marker_size: usize,
    pub text: Option<String>,
    pub versions: [Option<String>; 3],
    pub conflicts: Vec<Conflict>,
}

fn stages(index: &[u8], path: &str) -> Result<[bool; 4], String> {
    let mut stages = [false; 4];
    for record in index
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let entry = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Invalid conflict index entry.")?;
        let (header, name) = record.split_at(entry);
        if &name[1..] != path.as_bytes() {
            return Err("Select one file, not a directory.".into());
        }
        let mut fields = header.split(|byte| *byte == b' ');
        if !matches!(fields.next(), Some(b"100644" | b"100755" | b"120000")) {
            return Err("Conflict Solver supports regular files and symbolic links.".into());
        }
        let stage = match fields.nth(1) {
            Some(b"0") => 0,
            Some(b"1") => 1,
            Some(b"2") => 2,
            Some(b"3") => 3,
            _ => return Err("Invalid conflict index stage.".into()),
        };
        stages[stage] = true;
    }
    Ok(stages)
}

pub fn prepare(root: &Path, path: &str, action: Action) -> Result<Review, String> {
    let target = discard::safe_path(root, path)?;
    let index = discard::index_entry(root, path)?;
    let current = stages(&index, path)?;
    let unmerged = current[1..].contains(&true);
    let undo = if action == Action::Recreate && !unmerged {
        git::run(root, &["ls-files", "--resolve-undo", "-z", "--", path])?
    } else {
        Vec::new()
    };
    let stages = if undo.is_empty() {
        current
    } else {
        stages(&undo, path)?
    };
    if action == Action::Recreate {
        if !stages[2] || !stages[3] {
            return Err("Git has no pair of conflict versions to recreate for this file.".into());
        }
    } else if !unmerged {
        return Err("This file is no longer conflicted. Refresh the working tree.".into());
    }
    let file = discard::file_state(&target)?;
    let mut review = Review {
        index,
        undo,
        file,
        stages,
        marker_size: 7,
        text: None,
        versions: Default::default(),
        conflicts: Vec::new(),
    };
    if action == Action::Edit {
        let attr = git::run(
            root,
            &["check-attr", "-z", "conflict-marker-size", "--", path],
        )?;
        review.marker_size = attr
            .split(|byte| *byte == 0)
            .nth(2)
            .and_then(|value| std::str::from_utf8(value).ok())
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|size| *size > 0)
            .unwrap_or(7);
        if let discard::Contents::File(bytes) = &review.file.contents {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if !text.contains('\0') {
                    review.conflicts = parse(text, review.marker_size)?;
                    review.text = Some(text.to_owned());
                }
            }
        }
        if review.text.is_some() {
            for (version, stage) in review.versions.iter_mut().zip(1..=3) {
                if !review.stages[stage] {
                    continue;
                }
                let object = format!(":{stage}:{path}");
                let size = git::text(git::run(root, &["cat-file", "-s", &object])?)?;
                if size.trim().parse::<usize>().is_ok_and(|size| size <= LIMIT) {
                    *version = String::from_utf8(git::run(root, &["cat-file", "blob", &object])?)
                        .ok()
                        .filter(|text| !text.contains('\0'));
                }
            }
        }
    }
    Ok(review)
}

fn marker(line: &str, size: usize) -> Option<u8> {
    let line = line.trim_end_matches(['\r', '\n']).as_bytes();
    let &first = line.first()?;
    if !matches!(first, b'<' | b'|' | b'=' | b'>') {
        return None;
    }
    let count = line.iter().take_while(|&&byte| byte == first).count();
    (count == size && (count == line.len() || (first != b'=' && line[count] == b' ')))
        .then_some(first)
}

fn parse(text: &str, size: usize) -> Result<Vec<Conflict>, String> {
    let mut conflicts = Vec::new();
    let mut offset = 0;
    let mut start = None;
    let mut base = None;
    let mut separator = None;
    for line in text.split_inclusive('\n') {
        let end = offset + line.len();
        match (marker(line, size), start, base, separator) {
            (Some(b'<'), None, _, _) => start = Some((offset, end)),
            (Some(b'|'), Some(_), None, None) => base = Some((offset, end)),
            (Some(b'='), Some(_), _, None) => separator = Some((offset, end)),
            (Some(b'>'), Some((begin, ours)), base_line, Some((middle, theirs))) => {
                conflicts.push(Conflict {
                    span: begin..end,
                    ours: ours..base_line.map_or(middle, |(offset, _)| offset),
                    theirs: theirs..offset,
                });
                start = None;
                base = None;
                separator = None;
            }
            (Some(b'<'), Some(_), _, _) => return Err(marker_error()),
            _ => {}
        }
        offset = end;
    }
    if start.is_some() {
        return Err(marker_error());
    }
    Ok(conflicts)
}

fn marker_error() -> String {
    "The file contains incomplete or nested conflict markers. Use Take Ours or Take Theirs, or edit the file externally.".into()
}

#[cfg(test)]
fn combine(text: &str, conflicts: &[Conflict], edits: &[Option<String>]) -> Option<String> {
    if conflicts.len() != edits.len() {
        return None;
    }
    let mut result = String::new();
    let mut end = 0;
    for (conflict, edit) in conflicts.iter().zip(edits) {
        result.push_str(&text[end..conflict.span.start]);
        result.push_str(edit.as_deref()?);
        end = conflict.span.end;
    }
    result.push_str(&text[end..]);
    Some(result)
}

impl Review {
    pub fn can_stage(&self, text: &str) -> bool {
        text.len() <= LIMIT
            && !text.contains('\0')
            && parse(text, self.marker_size).is_ok_and(|conflicts| conflicts.is_empty())
    }
}

pub struct Comparison {
    pub rows: Vec<CompareRow>,
    pub conflict_rows: Vec<Range<usize>>,
    pub change_rows: [Vec<usize>; 3],
    pub sources: [String; 2],
    pub result: String,
    pub connectors: [Vec<Connector>; 3],
    pub applied_rows: Vec<Range<usize>>,
    original_context: Vec<(Range<usize>, usize)>,
}

#[derive(Clone)]
pub struct BlockEdit {
    pub original: Range<usize>,
    pub replacement: String,
    pub side: usize,
}

pub struct Connector {
    pub rows: Range<usize>,
    pub change: LineChange,
    pub conflict: Option<usize>,
}

pub struct CompareRow {
    pub lines: [Option<CompareLine>; 3],
    pub conflict: Option<usize>,
    pub marker: bool,
    // Each entry compares one Git side to the working tree.
    pub changes: [Option<LineChange>; 2],
    result_offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineChange {
    Added,
    Removed,
    Replaced,
}

pub struct CompareLine {
    pub number: usize,
    pub span: Range<usize>,
}

impl Comparison {
    pub fn new(
        text: &str,
        conflicts: &[Conflict],
        edits: &[Option<String>],
        versions: [Option<&str>; 2],
        block_edits: &[BlockEdit],
    ) -> Self {
        let sources = std::array::from_fn(|side| {
            versions[side].map(str::to_owned).unwrap_or_else(|| {
                let mut source = String::new();
                let mut end = 0;
                for block in conflicts {
                    source.push_str(&text[end..block.span.start]);
                    let range = if side == 0 {
                        &block.ours
                    } else {
                        &block.theirs
                    };
                    source.push_str(&text[range.clone()]);
                    end = block.span.end;
                }
                source.push_str(&text[end..]);
                source
            })
        });

        let mut result = String::with_capacity(text.len());
        let mut blocks = Vec::with_capacity(conflicts.len());
        let mut excluded = Vec::new();
        let mut applied = Vec::with_capacity(block_edits.len());
        let mut original_context = Vec::new();
        let mut patches = block_edits.iter().peekable();
        let mut append_context = |result: &mut String, mut start: usize, end: usize| {
            while let Some(patch) = patches.next_if(|patch| patch.original.end <= end) {
                let context_start = result.len();
                result.push_str(&text[start..patch.original.start]);
                original_context.push((context_start..result.len(), start));
                let applied_start = result.len();
                result.push_str(&patch.replacement);
                applied.push(applied_start..result.len());
                start = patch.original.end;
            }
            let context_start = result.len();
            result.push_str(&text[start..end]);
            original_context.push((context_start..result.len(), start));
        };
        let mut end = 0;
        for (id, (block, edit)) in conflicts.iter().zip(edits).enumerate() {
            append_context(&mut result, end, block.span.start);
            let start = result.len();
            if let Some(edit) = edit {
                result.push_str(edit);
            } else {
                result.push_str(&text[block.span.clone()]);
                // Keep marker rows in the editor and its line numbers, outside content comparison.
                excluded.extend([
                    start..start + block.ours.start - block.span.start,
                    start + block.ours.end - block.span.start
                        ..start + block.theirs.start - block.span.start,
                    start + block.theirs.end - block.span.start..result.len(),
                ]);
            }
            blocks.push((start..result.len(), id));
            end = block.span.end;
        }
        append_context(&mut result, end, text.len());

        let source_lines = sources.each_ref().map(|source| numbered_lines(source));
        let result_lines = numbered_lines(&result);
        let mut excluded = excluded.iter().peekable();
        let mut result_blocks = blocks.iter().peekable();
        let mut included = Vec::new();
        let mut row_info = Vec::with_capacity(result_lines.len());
        for (index, line) in result_lines.iter().enumerate() {
            let start = line.span.start;
            while excluded.peek().is_some_and(|span| span.end <= start) {
                excluded.next();
            }
            while result_blocks
                .peek()
                .is_some_and(|(span, _)| span.end <= start)
            {
                result_blocks.next();
            }
            let marker = excluded.peek().is_some_and(|span| span.contains(&start));
            let block = result_blocks
                .peek()
                .filter(|(span, _)| span.contains(&start))
                .map(|(_, id)| *id);
            row_info.push((start, marker, block));
            if !marker {
                included.push(index);
            }
        }

        let result_text: Vec<_> = included
            .iter()
            .map(|&index| &result[result_lines[index].span.clone()])
            .collect();
        let alignments: [SideAlignment; 2] = std::array::from_fn(|side| {
            let source: Vec<_> = source_lines[side]
                .iter()
                .map(|line| &sources[side][line.span.clone()])
                .collect();
            SideAlignment::new(&source, &result_text)
        });
        let mut source_lines =
            source_lines.map(|lines| lines.into_iter().map(Some).collect::<Vec<_>>());
        let mut rows = Vec::with_capacity(result_lines.len());
        let mut conflict_rows = vec![0..0; blocks.len()];
        let mut block_starts = blocks.iter().peekable();
        let mut content_index = 0;
        for item in result_lines
            .into_iter()
            .zip(row_info)
            .map(Some)
            .chain(std::iter::once(None))
        {
            let (start, marker, block) = item
                .as_ref()
                .map_or((result.len(), false, None), |(_, info)| *info);
            let mut empty_block = None;
            while let Some((span, id)) = block_starts.next_if(|(span, _)| span.start <= start) {
                conflict_rows[*id] = rows.len()..rows.len();
                if span.is_empty() && span.start == start {
                    empty_block = Some(*id);
                }
            }
            if !marker {
                let [mut ours, mut theirs] = alignments
                    .each_ref()
                    .map(|side| side.removed[content_index].clone());
                while !ours.is_empty() || !theirs.is_empty() {
                    let lines = [
                        ours.next().and_then(|index| source_lines[0][index].take()),
                        theirs
                            .next()
                            .and_then(|index| source_lines[1][index].take()),
                        None,
                    ];
                    let changes = std::array::from_fn(|side| {
                        lines[side].as_ref().map(|_| LineChange::Removed)
                    });
                    rows.push(CompareRow {
                        lines,
                        conflict: block.or(empty_block),
                        marker: false,
                        changes,
                        result_offset: start,
                    });
                }
            }
            if let Some((line, _)) = item {
                let mut changes = [None; 2];
                let [ours, theirs] = std::array::from_fn(|side| {
                    if marker {
                        return None;
                    }
                    match alignments[side].paired[content_index] {
                        Some((index, same)) => {
                            changes[side] = (!same).then_some(LineChange::Replaced);
                            source_lines[side][index].take()
                        }
                        None => {
                            changes[side] = Some(LineChange::Added);
                            None
                        }
                    }
                });
                rows.push(CompareRow {
                    lines: [ours, theirs, Some(line)],
                    conflict: block,
                    marker,
                    changes,
                    result_offset: start,
                });
                content_index += usize::from(!marker);
            }
            if let Some(id) = block.or(empty_block) {
                conflict_rows[id].end = rows.len();
            }
        }
        let change_rows = std::array::from_fn(|view| {
            let mut previous = None;
            rows.iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    let changed = row.marker
                        || match view {
                            0 | 1 => row.changes[view].is_some(),
                            _ => row.changes.iter().any(Option::is_some),
                        };
                    // Do not split a conflict at an unchanged row between its marker lines.
                    let group = row.conflict.map(Some).or_else(|| changed.then_some(None));
                    let start = group.is_some() && group != previous;
                    previous = group;
                    start.then_some(index)
                })
                .collect()
        });
        let applied_rows = applied
            .into_iter()
            .map(|range| {
                let start = rows.partition_point(|row| row.result_offset < range.start);
                let end = rows.partition_point(|row| row.result_offset < range.end);
                start..end
            })
            .collect();
        let mut comparison = Self {
            rows,
            conflict_rows,
            change_rows,
            sources,
            result,
            connectors: Default::default(),
            applied_rows,
            original_context,
        };
        comparison.connectors = std::array::from_fn(|pair| {
            let mut runs: Vec<Connector> = Vec::new();
            for (index, row) in comparison.rows.iter().enumerate() {
                let change = if pair < 2 {
                    row.changes[pair]
                } else {
                    comparison.between_sides(row)
                };
                if let Some(change) = change {
                    if let Some(last) = runs.last_mut().filter(|run| {
                        run.change == change
                            && run.rows.end == index
                            && run.conflict == row.conflict
                    }) {
                        last.rows.end += 1;
                    } else {
                        runs.push(Connector {
                            rows: index..index + 1,
                            change,
                            conflict: row.conflict,
                        });
                    }
                }
            }
            runs
        });
        comparison
    }

    pub fn block_target(&self, run: &Connector) -> Option<Range<usize>> {
        if run.conflict.is_some() {
            return None;
        }
        let rows = &self.rows[run.rows.clone()];
        let start = rows.first()?.result_offset;
        let end = rows.last()?.lines[2]
            .as_ref()
            .map_or(start, |line| line_end(&self.result, line));
        let (context, offset) = self
            .original_context
            .iter()
            .find(|(context, _)| context.start <= start && end <= context.end)?;
        Some(offset + start - context.start..offset + end - context.start)
    }

    pub fn block_edit(
        &self,
        run: &Connector,
        side: usize,
        original: &str,
        target: Range<usize>,
    ) -> Option<BlockEdit> {
        let rows = &self.rows[run.rows.clone()];
        let source = &self.sources[side];
        let mut replacement = match rows.first()?.lines[side].as_ref() {
            Some(first) => {
                let last = rows.last()?.lines[side].as_ref()?;
                source[first.span.start..line_end(source, last)].replace("\r\n", "\n")
            }
            None => String::new(),
        };
        let crlf = original
            .split_once('\n')
            .is_some_and(|(first, _)| first.ends_with('\r'));
        if !replacement.is_empty() && !replacement.ends_with('\n') && target.end < original.len() {
            replacement.push('\n');
        }
        if target.is_empty()
            && target.start == original.len()
            && original
                .as_bytes()
                .last()
                .is_some_and(|&byte| byte != b'\n')
            && !replacement.is_empty()
        {
            replacement.insert(0, '\n');
        }
        if crlf {
            replacement = replacement.replace('\n', "\r\n");
        }
        (original[target.clone()] != replacement).then_some(BlockEdit {
            original: target,
            replacement,
            side,
        })
    }

    pub fn between_sides(&self, row: &CompareRow) -> Option<LineChange> {
        match (&row.lines[0], &row.lines[1]) {
            (Some(left), Some(right)) => (self.sources[0][left.span.clone()]
                != self.sources[1][right.span.clone()])
            .then_some(LineChange::Replaced),
            (Some(_), None) => Some(LineChange::Removed),
            (None, Some(_)) => Some(LineChange::Added),
            (None, None) => None,
        }
    }
}

fn line_end(text: &str, line: &CompareLine) -> usize {
    let tail = &text[line.span.end..];
    line.span.end
        + if tail.starts_with("\r\n") {
            2
        } else {
            usize::from(tail.starts_with('\n'))
        }
}

fn numbered_lines(text: &str) -> Vec<CompareLine> {
    let mut offset = 0;
    text.split_inclusive('\n')
        .enumerate()
        .map(|(index, text)| {
            let start = offset;
            offset += text.len();
            CompareLine {
                number: index + 1,
                span: start..start + text.trim_end_matches(['\r', '\n']).len(),
            }
        })
        .collect()
}

struct SideAlignment {
    paired: Vec<Option<(usize, bool)>>,
    removed: Vec<Range<usize>>,
}

impl SideAlignment {
    fn new(source: &[&str], result: &[&str]) -> Self {
        let mut diff = Self {
            paired: vec![None; result.len()],
            removed: vec![0..0; result.len() + 1],
        };
        let (mut old, mut new) = (0, 0);
        let anchors = matching_lines(source, result);
        for (left, right) in anchors.into_iter().chain([(source.len(), result.len())]) {
            let replacements = (left - old).min(right - new);
            for index in 0..replacements {
                diff.paired[new + index] = Some((old + index, false));
            }
            diff.removed[new + replacements] = old + replacements..left;
            if right < result.len() {
                diff.paired[right] = Some((left, true));
            }
            old = left + 1;
            new = right + 1;
        }
        diff
    }
}

// Unique lines anchor large regions. Small gaps also match repeated lines
// through a bounded LCS, so isolated insertions do not shift later rows.
fn matching_lines(source: &[&str], result: &[&str]) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    let mut positions = HashMap::new();
    for (index, &text) in source.iter().enumerate() {
        positions
            .entry(text)
            .and_modify(|entry: &mut (Option<usize>, Option<usize>, bool)| entry.0 = None)
            .or_insert((Some(index), None, false));
    }
    for (index, &text) in result.iter().enumerate() {
        if let Some((_, position, duplicate)) = positions.get_mut(text) {
            *duplicate |= position.replace(index).is_some();
        }
    }
    let mut candidates = Vec::new();
    let mut ends: Vec<usize> = Vec::new();
    for (left, text) in source.iter().enumerate() {
        if let Some((Some(unique), Some(right), false)) = positions.get(text).copied() {
            if unique == left {
                let slot = ends.partition_point(|&index| {
                    let (_, end, _): (usize, usize, Option<usize>) = candidates[index];
                    end < right
                });
                let previous = slot.checked_sub(1).map(|index| ends[index]);
                let candidate = candidates.len();
                candidates.push((left, right, previous));
                if slot == ends.len() {
                    ends.push(candidate);
                } else {
                    ends[slot] = candidate;
                }
            }
        }
    }
    let mut anchors = Vec::new();
    let mut current = ends.last().copied();
    while let Some(index) = current {
        let (left, right, previous) = candidates[index];
        anchors.push((left, right));
        current = previous;
    }
    let mut matched = Vec::new();
    let mut budget = 1_000_000;
    let (mut old, mut new) = (0, 0);
    for (left, right) in anchors
        .into_iter()
        .rev()
        .chain([(source.len(), result.len())])
    {
        match_gap(
            source,
            result,
            old..left,
            new..right,
            &mut matched,
            &mut budget,
        );
        if left < source.len() {
            matched.push((left, right));
        }
        old = left + 1;
        new = right + 1;
    }
    matched
}

fn match_gap(
    source: &[&str],
    result: &[&str],
    mut old: Range<usize>,
    mut new: Range<usize>,
    matched: &mut Vec<(usize, usize)>,
    budget: &mut usize,
) {
    while !old.is_empty() && !new.is_empty() && source[old.start] == result[new.start] {
        matched.push((old.start, new.start));
        old.start += 1;
        new.start += 1;
    }
    let mut suffix = 0;
    while !old.is_empty() && !new.is_empty() && source[old.end - 1] == result[new.end - 1] {
        old.end -= 1;
        new.end -= 1;
        suffix += 1;
    }
    let stride = new.len() + 1;
    let cells = (old.len() + 1).saturating_mul(stride);
    if !old.is_empty() && !new.is_empty() && cells <= (*budget).min(65_536) {
        *budget -= cells;
        let mut lengths = vec![0_u32; cells];
        for left in (0..old.len()).rev() {
            for right in (0..new.len()).rev() {
                let cell = left * stride + right;
                lengths[cell] = if source[old.start + left] == result[new.start + right] {
                    lengths[cell + stride + 1] + 1
                } else {
                    lengths[cell + 1].max(lengths[cell + stride])
                };
            }
        }
        let (mut left, mut right) = (0, 0);
        while left < old.len() && right < new.len() {
            let cell = left * stride + right;
            if source[old.start + left] == result[new.start + right] {
                matched.push((old.start + left, new.start + right));
                left += 1;
                right += 1;
            } else if lengths[cell + stride] >= lengths[cell + 1] {
                left += 1;
            } else {
                right += 1;
            }
        }
    }
    matched.extend((0..suffix).map(|index| (old.end + index, new.end + index)));
}
pub fn apply(
    root: &Path,
    path: &str,
    action: Action,
    review: &Review,
    draft: Option<String>,
) -> Result<(), String> {
    let target = discard::safe_path(root, path)?;
    let index = discard::index_entry(root, path)?;
    let current = discard::file_state(&target)?;
    let same_draft = matches!((&current.contents, &draft), (discard::Contents::File(bytes), Some(draft)) if bytes == draft.as_bytes());
    if index != review.index || (current != review.file && !(action == Action::Edit && same_draft))
    {
        return Err(
            "The conflict or working copy changed. Reopen Conflict Solver and review it again."
                .into(),
        );
    }
    match action {
        Action::Edit => {
            if review.text.is_none() {
                return Err("The working copy has no editable text.".into());
            }
            let draft = draft.ok_or("Resolve every conflict block before staging.")?;
            if draft.len() > LIMIT || draft.contains('\0') {
                return Err("The result must be text and at most 2 MiB.".into());
            }
            if !parse(&draft, review.marker_size)?.is_empty() {
                return Err("Remove the remaining conflict markers before staging.".into());
            }
            if !same_draft {
                replace_file(&target, draft.as_bytes())?;
            }
            git::run(root, &["add", "--", path])?;
        }
        Action::Ours | Action::Theirs => {
            let (stage, option) = if action == Action::Ours {
                (2, "--ours")
            } else {
                (3, "--theirs")
            };
            if review.stages[stage] {
                git::run(root, &["checkout", option, "--", path])?;
                git::run(root, &["add", "--", path])?;
            } else {
                git::run(root, &["rm", "-f", "--", path])?;
            }
        }
        Action::Recreate => {
            if !review.undo.is_empty()
                && git::run(root, &["ls-files", "--resolve-undo", "-z", "--", path])? != review.undo
            {
                return Err("Git's conflict versions changed. Review this file again.".into());
            }
            git::run(root, &["checkout", "-m", "--", path])?;
        }
    }
    Ok(())
}

fn replace_file(target: &Path, contents: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let permissions = target
        .symlink_metadata()
        .map_err(|error| format!("Cannot inspect selected file: {error}"))?
        .permissions();
    let temporary = target.with_file_name(format!(
        ".sannygit-conflict-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Cannot create a temporary file for the resolution: {error}"))?;
    let written = file
        .write_all(contents)
        .and_then(|()| file.set_permissions(permissions));
    drop(file);
    let result = written.and_then(|()| fs::rename(&temporary, target));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Cannot save the resolved file: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempRepo;

    fn fixture(theirs: Option<&str>) -> TempRepo {
        let repo = TempRepo::new();
        repo.write("file", "start\nbase\nend\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "base"]);
        repo.git(&["checkout", "-b", "incoming"]);
        if let Some(theirs) = theirs {
            repo.write("file", theirs);
            repo.git(&["add", "file"]);
        } else {
            repo.git(&["rm", "file"]);
        }
        repo.git(&["commit", "-m", "incoming"]);
        repo.git(&["checkout", "main"]);
        repo.write("file", "start\nours\nend\n");
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-m", "ours"]);
        assert!(git::run(
            &repo.0,
            &["-c", "merge.conflictStyle=diff3", "merge", "incoming"]
        )
        .is_err());
        repo
    }

    #[test]
    fn combines_multiple_blocks_without_changing_surrounding_bytes() {
        let text = "start\r\n<<<< ours\r\na\r\n|||| base\r\nb\r\n====\r\nc\r\n>>>> theirs\r\nkeep\n<<<< ours\n====\nd\n>>>> theirs";
        let blocks = parse(text, 4).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            &text[blocks[0].ours.end..blocks[0].theirs.start],
            "|||| base\r\nb\r\n====\r\n"
        );
        assert_eq!(&text[blocks[1].ours.clone()], "");
        assert!(combine(text, &blocks, &[None, Some(String::new())]).is_none());
        assert_eq!(
            combine(
                text,
                &blocks,
                &[Some("a\r\nc\r\n".into()), Some(String::new())]
            )
            .unwrap(),
            "start\r\na\r\nc\r\nkeep\n"
        );
        assert!(parse("<<<< ours\n", 4).is_err());
        let preview = Comparison::new(
            text,
            &blocks,
            &[Some("a\r\nc\r\n".into()), None],
            [None; 2],
            &[],
        );
        assert!(preview.result.starts_with("start\r\na\r\nc\r\nkeep\n"));
        let first = preview.conflict_rows[0].start;
        assert_eq!(preview.rows[first].conflict, Some(0));
        let [ours, _, result] = &preview.rows[first].lines;
        assert_eq!(
            &preview.sources[0][ours.as_ref().unwrap().span.clone()],
            "a"
        );
        assert_eq!(result.as_ref().unwrap().number, 2);
        assert!(preview.rows[first + 1].lines[0].is_none());
        assert!(preview
            .rows
            .iter()
            .filter(|row| row.marker)
            .all(|row| row.changes == [None; 2]));
    }

    #[test]
    fn compares_actual_git_sides_and_keeps_unchanged_lines_aligned() {
        use LineChange::{Added, Removed, Replaced};
        let ours = "Start\nRemove this\nRepeated\nOld wording\nRepeated\nLast line\n";
        let result = "Start\nRepeated\nNew wording\nRepeated\nInsert this\nLast line\n";
        let theirs = "Start\nRepeated\nNew wording\nRepeated\nInsert this\nLast line\n";
        let comparison = Comparison::new(result, &[], &[], [Some(ours), Some(theirs)], &[]);
        assert_eq!(
            comparison
                .rows
                .iter()
                .map(|row| row.changes)
                .collect::<Vec<_>>(),
            [
                [None, None],
                [Some(Removed), None],
                [None, None],
                [Some(Replaced), None],
                [None, None],
                [Some(Added), None],
                [None, None]
            ]
        );
        let numbers = comparison
            .rows
            .last()
            .unwrap()
            .lines
            .each_ref()
            .map(|line| line.as_ref().unwrap().number);
        assert_eq!(numbers, [6, 6, 6]);
    }

    #[test]
    fn connector_edits_apply_full_blocks_and_can_be_undone_independently() {
        let result = "head\r\ninserted\r\nstable a\r\nnew\r\nstable b\r\ntail\r\n";
        let ours = "head\nstable a\nold\nstable b\nrestored\ntail\n";
        let compare = |edits: &[BlockEdit]| {
            Comparison::new(result, &[], &[], [Some(ours), Some(result)], edits)
        };
        let initial = compare(&[]);
        let mut edits: Vec<_> = initial.connectors[0]
            .iter()
            .map(|run| {
                let target = initial.block_target(run).unwrap();
                initial.block_edit(run, 0, result, target).unwrap()
            })
            .collect();
        assert_eq!(edits.len(), 3);
        let changed = compare(&edits);
        assert_eq!(changed.result, ours.replace('\n', "\r\n"));
        assert!(changed.connectors[0].is_empty());
        assert_eq!(changed.applied_rows.len(), 3);
        edits.remove(1);
        assert_eq!(
            compare(&edits).result,
            "head\r\nstable a\r\nnew\r\nstable b\r\nrestored\r\ntail\r\n"
        );
    }

    #[test]
    fn undo_from_a_connector_restores_the_previous_choice_for_that_conflict() {
        use crate::app::conflict::Dialog;
        let repo = fixture(Some("start\ntheirs\nend\n"));
        let mut dialog = Dialog::new(repo.0.clone(), "file".into(), Action::Edit);
        dialog.set_review(std::sync::Arc::new(
            prepare(&repo.0, "file", Action::Edit).unwrap(),
        ));
        dialog.take_conflict(0, 0);
        assert!(dialog.ready());
        assert_eq!(
            dialog.comparison.as_ref().unwrap().result,
            "start\nours\nend\n"
        );
        dialog.take_conflict(0, 1);
        assert_eq!(
            dialog.comparison.as_ref().unwrap().result,
            "start\ntheirs\nend\n"
        );
        dialog.undo_conflict(0);
        assert_eq!(
            dialog.comparison.as_ref().unwrap().result,
            "start\nours\nend\n"
        );
        assert_eq!(
            fs::read_to_string(repo.0.join("file")).unwrap(),
            dialog.review.as_ref().unwrap().text.as_deref().unwrap()
        );
    }

    #[test]
    fn saves_and_stages_the_reviewed_result_then_recreates_the_conflict() {
        let repo = fixture(Some("start\ntheirs\nend\n"));
        repo.write("unrelated", "keep\n");
        let review = prepare(&repo.0, "file", Action::Edit).unwrap();
        assert_eq!(review.conflicts.len(), 1);
        let draft = combine(
            review.text.as_deref().unwrap(),
            &review.conflicts,
            &[Some("combined\n".into())],
        );
        apply(&repo.0, "file", Action::Edit, &review, draft).unwrap();
        assert_eq!(repo.git(&["show", ":file"]), "start\ncombined\nend\n");
        assert_eq!(fs::read(repo.0.join("unrelated")).unwrap(), b"keep\n");
        assert!(repo.git(&["ls-files", "-u"]).is_empty());
        let review = prepare(&repo.0, "file", Action::Recreate).unwrap();
        apply(&repo.0, "file", Action::Recreate, &review, None).unwrap();
        assert!(!repo.git(&["ls-files", "-u"]).is_empty());
        assert!(fs::read_to_string(repo.0.join("file"))
            .unwrap()
            .contains("<<<<<<<"));
    }

    #[test]
    fn refuses_a_stale_review_and_can_accept_the_deleted_side() {
        let repo = fixture(None);
        let stale = prepare(&repo.0, "file", Action::Theirs).unwrap();
        repo.write("file", "external edit\n");
        assert!(apply(&repo.0, "file", Action::Theirs, &stale, None).is_err());
        assert_eq!(fs::read(repo.0.join("file")).unwrap(), b"external edit\n");
        let current = prepare(&repo.0, "file", Action::Theirs).unwrap();
        apply(&repo.0, "file", Action::Theirs, &current, None).unwrap();
        assert!(!repo.0.join("file").exists());
        assert!(repo.git(&["ls-files", "--", "file"]).is_empty());
    }

    #[test]
    fn takes_the_incoming_version_and_stages_only_the_selected_file() {
        let repo = fixture(Some("start\ntheirs\nend\n"));
        repo.write("unrelated", "keep\n");
        let review = prepare(&repo.0, "file", Action::Theirs).unwrap();
        apply(&repo.0, "file", Action::Theirs, &review, None).unwrap();
        assert_eq!(repo.git(&["show", ":file"]), "start\ntheirs\nend\n");
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "file\n");
    }

    #[test]
    fn solver_controls_fit_in_both_themes() {
        use crate::{
            app::conflict::{Dialog, Layout},
            theme::{install_fonts, ThemeChoice},
        };
        use eframe::egui::{
            Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2,
        };
        use std::sync::Arc;

        let repo = fixture(Some("start\ntheirs\nend\n"));
        let working = fs::read_to_string(repo.0.join("file")).unwrap();
        repo.write("file", &format!("inserted header\n{working}"));
        let review = Arc::new(prepare(&repo.0, "file", Action::Edit).unwrap());
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [
                Vec2::new(890.0, 560.0),
                Vec2::new(1320.0, 850.0),
                Vec2::new(1536.0, 1024.0),
            ] {
                let ctx = Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = crate::app::SannyGitApp::with_theme(theme);
                let mut dialog = Dialog::new(repo.0.clone(), "file".into(), Action::Edit);
                dialog.set_review(Arc::clone(&review));
                app.conflict_dialog = Some(dialog);
                let bounds = Rect::from_min_size(Pos2::ZERO, size);
                let render = |app: &mut crate::app::SannyGitApp, events| {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(bounds),
                            events,
                            ..Default::default()
                        },
                        |ui| crate::ui::show(ui, app),
                    );
                    output.textures_delta.clear();
                    output
                };
                render(&mut app, Vec::new());
                let output = render(&mut app, Vec::new());
                let position = |label, toolbar_y: Option<f32>, rightmost: bool| {
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Text(text) if text.galley.job.text == label => {
                                let rect = text.galley.rect.translate(text.pos.to_vec2());
                                (bounds.intersect(shape.clip_rect).contains_rect(rect)
                                    && toolbar_y.is_none_or(|y| (rect.center().y - y).abs() < 2.0))
                                .then(|| rect.center())
                            }
                            _ => None,
                        })
                        .min_by(|left, right| {
                            left.y.total_cmp(&right.y).then_with(|| {
                                if rightmost {
                                    right.x.total_cmp(&left.x)
                                } else {
                                    left.x.total_cmp(&right.x)
                                }
                            })
                        })
                        .unwrap_or_else(|| panic!("missing or clipped {label} at {size:?}"))
                };
                let status = position("Ready", None, false);
                let (line, column) = app
                    .conflict_dialog
                    .as_ref()
                    .unwrap()
                    .cursor_position
                    .unwrap();
                let coordinates = format!("{line}:{column}");
                let caret = position(&coordinates, Some(status.y), true);
                assert!(
                    status.x < 70.0 && status.y > size.y - 55.0,
                    "status at {status:?} in {size:?}"
                );
                assert!(caret.x > size.x - 65.0, "coordinates at {caret:?}");
                let save = position("Save", None, false);
                let compact = size.x < 1000.0;
                let change_prev = if compact { "Prev." } else { "Prev. Change" };
                let change_next = if compact { "Next" } else { "Next Change" };
                let take_left = if compact { "Left" } else { "Take Left" };
                let base_label = if compact { "Base" } else { "Base Changes" };
                let merge_left = if compact { "L + Merge" } else { "Left + Merge" };
                let merge_right = if compact {
                    "Merge + R"
                } else {
                    "Merge + Right"
                };
                let merge_below = if compact { "Below" } else { "Merge Below" };
                let take_both = if compact {
                    "L → R"
                } else {
                    "Take Left, Right"
                };
                let take_right = if compact { "Right" } else { "Take Right" };
                let take_both_reversed = if compact {
                    "R → L"
                } else {
                    "Take Right, Left"
                };
                let captions = [
                    base_label,
                    "Save",
                    change_prev,
                    change_next,
                    take_both,
                    take_left,
                    take_right,
                    take_both_reversed,
                    merge_left,
                    "All",
                    merge_right,
                    merge_below,
                ];
                let mut previous = f32::NEG_INFINITY;
                for caption in captions {
                    let current = position(caption, Some(save.y), false);
                    assert!(previous < current.x);
                    previous = current.x;
                }
                let conflict_next = if compact { "Next" } else { "Next Conflict" };
                let conflict_prev = if compact { "Prev." } else { "Prev. Conflict" };
                let next_conflict = position(conflict_next, Some(save.y), true);
                assert!(next_conflict.x > previous && next_conflict.x > size.x - 110.0);
                assert!(save.y < position("Working Tree", None, false).y);
                let pitch = save.x - position(base_label, Some(save.y), false).x;
                for (left, right, rightmost) in [
                    (change_prev, change_next, false),
                    (take_both, take_left, false),
                    (take_left, take_right, false),
                    (take_right, take_both_reversed, false),
                    (merge_left, "All", false),
                    ("All", merge_right, false),
                    (merge_right, merge_below, false),
                    (conflict_prev, conflict_next, true),
                ] {
                    let gap = position(right, Some(save.y), rightmost).x
                        - position(left, Some(save.y), rightmost).x;
                    assert!(
                        (gap - pitch).abs() < 1.0,
                        "{left} and {right} have a different pitch at {size:?}"
                    );
                }

                let click = |app: &mut crate::app::SannyGitApp, pos| {
                    render(app, vec![Event::PointerMoved(pos)]);
                    for pressed in [true, false] {
                        render(
                            app,
                            vec![
                                Event::PointerMoved(pos),
                                Event::PointerButton {
                                    pos,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ],
                        );
                    }
                };
                click(&mut app, position("inserted header", None, false));
                let clicked = app.conflict_dialog.as_ref().unwrap().cursor_position;
                assert!(matches!(clicked, Some((1, 7..=10))), "got {clicked:?}");
                for (caption, layout) in [
                    ("All", Layout::ThreeWay),
                    (merge_right, Layout::ResultTheirs),
                    (merge_below, Layout::ResultBelow),
                    (merge_left, Layout::OursResult),
                ] {
                    click(&mut app, position(caption, Some(save.y), false));
                    assert!(
                        app.conflict_dialog.as_ref().unwrap().layout == layout,
                        "{caption} did not switch layout"
                    );
                }
                click(&mut app, position(change_prev, Some(save.y), false));
                assert_eq!(app.conflict_dialog.as_ref().unwrap().change_row, Some(0));
                click(&mut app, position(take_left, Some(save.y), false));
                let dialog = app.conflict_dialog.as_ref().unwrap();
                let merge_text = dialog.comparison.as_ref().unwrap().result.clone();
                assert!(!merge_text.contains("inserted header"));
                assert!(merge_text.contains("<<<<<<<"));
                let base = position(base_label, Some(save.y), false);
                click(&mut app, base);
                let dialog = app.conflict_dialog.as_ref().unwrap();
                assert!(dialog.show_base);
                assert_eq!(
                    dialog.comparison.as_ref().unwrap().result,
                    "start\nbase\nend\n"
                );
                assert!(!dialog.ready());
                click(&mut app, base);
                let dialog = app.conflict_dialog.as_ref().unwrap();
                assert!(!dialog.show_base);
                assert_eq!(dialog.comparison.as_ref().unwrap().result, merge_text);
            }
        }
    }
}
