use crate::models::{DiffLine, DiffLineKind};

pub fn parse_unified_diff(input: &str) -> Vec<DiffLine> {
    let mut old_line = None;
    let mut new_line = None;

    input
        .lines()
        .map(|line| {
            if line.starts_with("diff --git ") {
                old_line = None;
                new_line = None;
                return header_line(line);
            }
            if line.starts_with("@@") {
                let numbers = parse_hunk_header(line);
                old_line = numbers.map(|(old, _)| old);
                new_line = numbers.map(|(_, new)| new);
                return DiffLine {
                    kind: DiffLineKind::Hunk,
                    old_line: None,
                    new_line: None,
                    text: line.to_owned(),
                };
            }

            if let Some(text) = line.strip_prefix('+') {
                if new_line.is_none() && line.starts_with("+++") {
                    return header_line(line);
                }
                let current = new_line;
                new_line = new_line.map(|number| number + 1);
                return DiffLine {
                    kind: DiffLineKind::Added,
                    old_line: None,
                    new_line: current,
                    text: text.to_owned(),
                };
            }

            if let Some(text) = line.strip_prefix('-') {
                if old_line.is_none() && line.starts_with("---") {
                    return header_line(line);
                }
                let current = old_line;
                old_line = old_line.map(|number| number + 1);
                return DiffLine {
                    kind: DiffLineKind::Removed,
                    old_line: current,
                    new_line: None,
                    text: text.to_owned(),
                };
            }

            if let Some(text) = line.strip_prefix(' ') {
                let current_old = old_line;
                let current_new = new_line;
                old_line = old_line.map(|number| number + 1);
                new_line = new_line.map(|number| number + 1);
                return DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: current_old,
                    new_line: current_new,
                    text: text.to_owned(),
                };
            }

            header_line(line)
        })
        .collect()
}

fn header_line(line: &str) -> DiffLine {
    DiffLine {
        kind: DiffLineKind::Header,
        old_line: None,
        new_line: None,
        text: line.to_owned(),
    }
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let mut old = None;
    let mut new = None;

    for token in line.split_whitespace() {
        if token.starts_with('-') {
            old = parse_start_line(token);
        } else if token.starts_with('+') {
            new = parse_start_line(token);
        }
    }

    old.zip(new)
}

fn parse_start_line(token: &str) -> Option<u32> {
    token.get(1..)?.split(',').next()?.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_numbers_for_all_line_kinds() {
        let lines = parse_unified_diff(
            "@@ -10,2 +12,3 @@ example\n context\n-removed\n+added one\n+added two",
        );

        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].kind, DiffLineKind::Hunk);
        assert_eq!((lines[1].old_line, lines[1].new_line), (Some(10), Some(12)));
        assert_eq!((lines[2].old_line, lines[2].new_line), (Some(11), None));
        assert_eq!((lines[3].old_line, lines[3].new_line), (None, Some(13)));
        assert_eq!((lines[4].old_line, lines[4].new_line), (None, Some(14)));
    }

    #[test]
    fn preserves_header_like_content_and_resets_numbers_between_patches() {
        let lines = parse_unified_diff("@@ -1 +1 @@\n---old code\n+++new code\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -4 +5 @@\n context");
        assert_eq!(lines[1].kind, DiffLineKind::Removed);
        assert_eq!(lines[1].text, "--old code");
        assert_eq!(lines[2].kind, DiffLineKind::Added);
        assert_eq!(lines[2].text, "++new code");
        assert_eq!(lines[4].kind, DiffLineKind::Header);
        assert_eq!(lines[5].kind, DiffLineKind::Header);
        assert_eq!((lines[7].old_line, lines[7].new_line), (Some(4), Some(5)));
    }

    #[test]
    fn keeps_file_headers_without_line_numbers() {
        let lines = parse_unified_diff("--- a/file.rs\n+++ b/file.rs\n@@ invalid");

        assert_eq!(lines.len(), 3);
        assert!(lines
            .iter()
            .all(|line| line.old_line.is_none() && line.new_line.is_none()));
        assert_eq!(lines[2].kind, DiffLineKind::Hunk);
    }
}
