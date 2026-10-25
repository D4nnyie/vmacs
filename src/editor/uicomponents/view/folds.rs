use super::Line;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fold {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Hidden {
    ranges: Vec<(usize, usize)>,
}

pub fn attach_brace_lines(folds: Vec<Fold>, lines: &[Line]) -> Vec<Fold> {
    let mut by_start: BTreeMap<usize, usize> = BTreeMap::new();
    for fold in folds {
        let brace_alone = fold.start > 0
            && lines.get(fold.start).is_some_and(|l| l.trim() == "{")
            && lines
                .get(fold.start.saturating_sub(1))
                .is_some_and(|l| !l.trim().is_empty());
        let (start, end) = if brace_alone {
            let closing = lines
                .get(fold.end.saturating_add(1))
                .is_some_and(|l| is_closing_line(l));
            (
                fold.start.saturating_sub(1),
                if closing { fold.end.saturating_add(1) } else { fold.end },
            )
        } else {
            (fold.start, fold.end)
        };
        let entry = by_start.entry(start).or_insert(end);
        *entry = (*entry).max(end);
    }
    by_start
        .into_iter()
        .map(|(start, end)| Fold { start, end })
        .collect()
}

pub fn brace_folds(lines: &[Line]) -> Vec<Fold> {
    let mut stack: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut in_str = false;
        let mut prev = '\0';
        for c in line.chars() {
            if in_str {
                if c == '"' && prev != '\\' {
                    in_str = false;
                }
                prev = if prev == '\\' { '\0' } else { c };
                continue;
            }
            match c {
                '"' => in_str = true,
                '/' if prev == '/' => break,
                '{' => stack.push(i),
                '}' => {
                    if let Some(start) = stack.pop() {
                        let end = if is_closing_line(line) { i.saturating_sub(1) } else { i };
                        if end > start {
                            out.push(Fold { start, end });
                        }
                    }
                }
                _ => {}
            }
            prev = c;
        }
    }
    out
}

impl Hidden {
    pub fn new(mut ranges: Vec<(usize, usize)>) -> Self {
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in ranges {
            match merged.last_mut() {
                Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        Self { ranges: merged }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn range_containing(&self, line: usize) -> Option<(usize, usize)> {
        let i = self.ranges.partition_point(|&(_, e)| e < line);
        self.ranges.get(i).copied().filter(|&(s, _)| s <= line)
    }

    #[allow(dead_code)]
    pub fn is_hidden(&self, line: usize) -> bool {
        self.range_containing(line).is_some()
    }

    pub fn count_hidden(&self) -> usize {
        self.ranges.iter().map(|&(s, e)| e.saturating_sub(s).saturating_add(1)).sum()
    }

    pub fn vrow(&self, line: usize) -> usize {
        let hidden: usize = self
            .ranges
            .iter()
            .take_while(|&&(s, _)| s < line)
            .map(|&(s, e)| e.min(line.saturating_sub(1)).saturating_sub(s).saturating_add(1))
            .sum();
        line.saturating_sub(hidden)
    }

    pub fn line_of_vrow(&self, vrow: usize) -> usize {
        let mut line = vrow;
        for &(s, e) in &self.ranges {
            if line >= s {
                line = line.saturating_add(e.saturating_sub(s)).saturating_add(1);
            } else {
                break;
            }
        }
        line
    }
}

pub fn is_closing_line(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with(['}', ')', ']'])
        || matches!(
            t.split(|c: char| !c.is_alphanumeric()).next(),
            Some("fi" | "done" | "esac")
        )
}

pub fn indent_folds(lines: &[Line]) -> Vec<Fold> {
    let indent = |line: &Line| -> Option<usize> {
        if line.trim().is_empty() {
            return None;
        }
        Some(
            line.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum(),
        )
    };
    let mut folds = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut last_nonblank = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let Some(level) = indent(line) else {
            continue;
        };
        while stack.last().is_some_and(|&(_, top)| top >= level) {
            if let Some((start, _)) = stack.pop() {
                if last_nonblank > start {
                    folds.push(Fold { start, end: last_nonblank });
                }
            }
        }
        stack.push((i, level));
        last_nonblank = i;
    }
    while let Some((start, _)) = stack.pop() {
        if last_nonblank > start {
            folds.push(Fold { start, end: last_nonblank });
        }
    }
    folds.sort_by_key(|fold| fold.start);
    folds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_ranges_map_lines_and_rows() {
        let hidden = Hidden::new(vec![(3, 5), (6, 7)]);
        assert!(hidden.is_hidden(4) && hidden.is_hidden(7) && !hidden.is_hidden(8));
        assert_eq!(hidden.count_hidden(), 5);
        assert_eq!(hidden.vrow(8), 3);
        assert_eq!(hidden.line_of_vrow(3), 8);
        assert_eq!(hidden.line_of_vrow(2), 2);
    }

    #[test]
    fn indentation_makes_folds() {
        let lines: Vec<Line> = ["a", "  b", "  c", "d", "  e"].iter().map(|s| Line::from(s)).collect();
        assert_eq!(
            indent_folds(&lines),
            vec![Fold { start: 0, end: 2 }, Fold { start: 3, end: 4 }]
        );
    }

    #[test]
    fn closing_lines() {
        assert!(is_closing_line("   }"));
        assert!(is_closing_line("fi"));
        assert!(!is_closing_line("file"));
    }
}