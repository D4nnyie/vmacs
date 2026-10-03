use super::super::super::{AnnotatedString, Annotation};
use super::tscache::TsCache;
use super::FileInfo;
use super::Highlighter;
use super::Line;
use crate::prelude::*;
use std::fs::{read_to_string, File};
use std::io::Error;
use std::io::Write;
use std::ops::Range;

const MAX_TREE_SITTER_LINES: usize = 150_000;

#[derive(Default)]
pub struct Buffer {
    lines: Vec<Line>,
    file_info: FileInfo,
    dirty: bool,
    revision: u64,
    ts_cache: Option<TsCache>,
    ts_failed: bool,
}

impl Buffer {
    fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub const fn get_file_info(&self) -> &FileInfo {
        &self.file_info
    }

    pub fn refresh_syntax(&mut self) {
        if self.ts_failed || self.lines.len() > MAX_TREE_SITTER_LINES {
            return;
        }
        let Some(language) = self.file_info.get_language() else {
            return;
        };
        let Some(ts_config) = language.ts else {
            return;
        };
        if self.ts_cache.is_none() {
            match TsCache::new(&ts_config) {
                Some(cache) => self.ts_cache = Some(cache),
                None => {
                    self.ts_failed = true;
                    return;
                }
            }
        }
        if let Some(cache) = self.ts_cache.as_mut() {
            cache.update(&self.lines, self.revision);
        }
    }

    pub fn ts_annotations(&self) -> Option<&[Vec<Annotation>]> {
        self.ts_cache
            .as_ref()
            .filter(|cache| cache.is_current(self.revision))
            .map(TsCache::annotations)
    }

    pub fn autoindent_enabled(&self) -> bool {
        self.file_info
            .get_language()
            .is_some_and(|lang| !lang.indent_triggers.is_empty() || lang.ts.is_some())
    }

    pub fn newline_indent_level(&self, at: Location) -> usize {
        let Some(line) = self.lines.get(at.line_idx) else {
            return 0;
        };
        let prev_level = indent_level(&leading_whitespace(line));
        if !self.autoindent_enabled() {
            return prev_level;
        }
        let Some(language) = self.file_info.get_language() else {
            return prev_level;
        };

        let before = line.substring(0..at.grapheme_idx);
        let before = before.trim_end();
        let trigger = language
            .indent_triggers
            .iter()
            .any(|trigger| before.ends_with(*trigger));

        let tree = language.ts.and_then(|ts| {
            let cache = self
                .ts_cache
                .as_ref()
                .filter(|cache| cache.is_current(self.revision))?;
            let line_start = cache.line_start(at.line_idx)?;
            let byte_in_line = line
                .byte_range_for_grapheme_range(at.grapheme_idx..at.grapheme_idx)
                .start;
            cache.scope_level(line_start.saturating_add(byte_in_line), ts.indent_kinds)
        });

        match tree {
            Some((tree_level, has_error)) => {
                if trigger {
                    tree_level.max(prev_level.saturating_add(1))
                } else if tree_level > 0 && !has_error {
                    tree_level
                } else {
                    tree_level.max(prev_level)
                }
            }
            None => {
                if trigger {
                    prev_level.saturating_add(1)
                } else {
                    prev_level
                }
            }
        }
    }

    pub fn indent_level_for_open_above(&self, line_idx: LineIdx) -> usize {
        let Some(line) = self.lines.get(line_idx) else {
            return 0;
        };
        let level = indent_level(&leading_whitespace(line));
        let starts_with_closer = line
            .trim_start()
            .starts_with(|c| matches!(c, '}' | ')' | ']'));
        if self.autoindent_enabled() && starts_with_closer {
            level.saturating_sub(1)
        } else {
            level
        }
    }

    pub fn is_between_pair(&self, at: Location) -> bool {
        if at.grapheme_idx == 0 {
            return false;
        }
        let Some(line) = self.lines.get(at.line_idx) else {
            return false;
        };
        let before = line.grapheme_at(at.grapheme_idx.saturating_sub(1));
        let after = line.grapheme_at(at.grapheme_idx);
        matches!(
            (before, after),
            (Some("{"), Some("}")) | (Some("("), Some(")")) | (Some("["), Some("]"))
        )
    }

    pub fn matching_open_indent(&self, at: Location, closer: char) -> Option<String> {
        let opener = match closer {
            '}' => '{',
            ')' => '(',
            ']' => '[',
            _ => return None,
        };
        let mut depth = 0usize;
        let mut line_idx = at.line_idx;
        let mut end = at.grapheme_idx;
        loop {
            let line = self.lines.get(line_idx)?;
            let mut g = end.min(line.grapheme_count());
            while g > 0 {
                g = g.saturating_sub(1);
                let Some(grapheme) = line.grapheme_at(g) else {
                    continue;
                };
                let mut chars = grapheme.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    continue;
                };
                if c == closer {
                    depth = depth.saturating_add(1);
                } else if c == opener {
                    if depth == 0 {
                        return Some(leading_whitespace(line));
                    }
                    depth = depth.saturating_sub(1);
                }
            }
            if line_idx == 0 {
                return None;
            }
            line_idx = line_idx.saturating_sub(1);
            end = self.lines.get(line_idx)?.grapheme_count();
        }
    }

    pub fn set_leading_whitespace(
        &mut self,
        line_idx: LineIdx,
        new_whitespace: &str,
    ) -> Option<(GraphemeIdx, GraphemeIdx)> {
        let line = self.lines.get_mut(line_idx)?;
        let mut old = 0;
        while matches!(line.grapheme_at(0), Some(" ") | Some("\t")) {
            line.delete(0);
            old += 1;
        }
        let mut new = 0;
        for character in new_whitespace.chars() {
            line.insert_char(character, new);
            new += 1;
        }
        self.touch();
        Some((old, new))
    }

    pub fn line_content(&self, idx: LineIdx) -> Option<String> {
        self.lines.get(idx).map(std::string::ToString::to_string)
    }

    pub fn grapheme_count(&self, idx: LineIdx) -> GraphemeIdx {
        self.lines.get(idx).map_or(0, Line::grapheme_count)
    }
    pub fn width_until(&self, idx: LineIdx, until: GraphemeIdx) -> GraphemeIdx {
        self.lines
            .get(idx)
            .map_or(0, |line| line.width_until(until))
    }

    pub fn delete_line(&mut self, line_idx: LineIdx) {
        if line_idx < self.lines.len() {
            self.lines.remove(line_idx);
            self.touch();
        }
    }

    pub fn grapheme_at(&self, at: Location) -> Option<String> {
        self.lines
            .get(at.line_idx)
            .and_then(|line| line.grapheme_at(at.grapheme_idx))
            .map(str::to_string)
    }

    pub fn delete_to_line_end(&mut self, at: Location) {
        if let Some(line) = self.lines.get_mut(at.line_idx) {
            let count = line.grapheme_count();
            for _ in at.grapheme_idx..count {
                line.delete(at.grapheme_idx);
            }
            self.touch();
        }
    }

    pub fn insert_line(&mut self, idx: LineIdx, content: &str) {
        let idx = idx.min(self.lines.len());
        self.lines.insert(idx, Line::from(content));
        self.touch();
    }

    pub fn find_next_word_start(&self, from: Location) -> Location {
        let mut line_idx = from.line_idx;
        let mut grapheme_idx = from.grapheme_idx;

        while let Some(line) = self.lines.get(line_idx) {
            match line.grapheme_at(grapheme_idx) {
                Some(g) if !g.trim().is_empty() => grapheme_idx = grapheme_idx.saturating_add(1),
                _ => break,
            }
        }
        loop {
            let Some(line) = self.lines.get(line_idx) else {
                return Location { line_idx, grapheme_idx: 0 };
            };
            if grapheme_idx >= line.grapheme_count() {
                if line_idx.saturating_add(1) >= self.height() {
                    return Location { line_idx, grapheme_idx: line.grapheme_count() };
                }
                line_idx = line_idx.saturating_add(1);
                grapheme_idx = 0;
                continue;
            }
            match line.grapheme_at(grapheme_idx) {
                Some(g) if g.trim().is_empty() => grapheme_idx = grapheme_idx.saturating_add(1),
                _ => return Location { line_idx, grapheme_idx },
            }
        }
    }

    pub fn find_prev_word_start(&self, from: Location) -> Location {
        let mut line_idx = from.line_idx;
        let mut grapheme_idx = from.grapheme_idx;

        loop {
            if grapheme_idx == 0 {
                if line_idx == 0 {
                    return Location { line_idx: 0, grapheme_idx: 0 };
                }
                line_idx = line_idx.saturating_sub(1);
                grapheme_idx = self.lines.get(line_idx).map_or(0, Line::grapheme_count);
                continue;
            }
            let Some(line) = self.lines.get(line_idx) else {
                return Location { line_idx, grapheme_idx: 0 };
            };
            match line.grapheme_at(grapheme_idx.saturating_sub(1)) {
                Some(g) if g.trim().is_empty() => grapheme_idx = grapheme_idx.saturating_sub(1),
                _ => break,
            }
        }
        loop {
            if grapheme_idx == 0 {
                break;
            }
            let Some(line) = self.lines.get(line_idx) else { break };
            match line.grapheme_at(grapheme_idx.saturating_sub(1)) {
                Some(g) if !g.trim().is_empty() => grapheme_idx = grapheme_idx.saturating_sub(1),
                _ => break,
            }
        }
        Location { line_idx, grapheme_idx }
    }

    pub fn get_highlighted_substring(
        &self,
        line_idx: LineIdx,
        range: Range<GraphemeIdx>,
        highlighter: &Highlighter,
    ) -> Option<AnnotatedString> {
        self.lines.get(line_idx).map(|line| {
            line.get_annotated_visible_substr(range, Some(&highlighter.get_annotations(line_idx, line)))
        })
    }

    pub fn highlight(&self, idx: LineIdx, highlighter: &mut Highlighter) {
        if let Some(line) = self.lines.get(idx) {
            highlighter.highlight(idx, line);
        }
    }

    pub fn load(file_name: &str) -> Result<Self, Error> {
        let contents = read_to_string(file_name)?;
        let mut lines = Vec::new();
        for value in contents.lines() {
            lines.push(Line::from(value));
        }
        Ok(Self {
            lines,
            file_info: FileInfo::from(file_name),
            ..Self::default()
        })
    }

    pub fn search_forward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let mut is_first = true;
        for (line_idx, line) in self
            .lines
            .iter()
            .enumerate()
            .cycle()
            .skip(from.line_idx)
            .take(self.lines.len().saturating_add(1))
        {
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                0
            };
            if let Some(grapheme_idx) = line.search_forward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
        }
        None
    }
    pub fn search_backward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let mut is_first = true;
        for (line_idx, line) in self
            .lines
            .iter()
            .enumerate()
            .rev()
            .cycle()
            .skip(
                self.lines
                    .len()
                    .saturating_sub(from.line_idx)
                    .saturating_sub(1),
            )
            .take(self.lines.len().saturating_add(1))
        {
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                line.grapheme_count()
            };
            if let Some(grapheme_idx) = line.search_backward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
        }
        None
    }
    fn save_to_file(&self, file_info: &FileInfo) -> Result<(), Error> {
        if let Some(file_path) = file_info.get_path() {
            let mut file = File::create(file_path)?;
            for line in &self.lines {
                writeln!(file, "{line}")?;
            }
            Ok(())
        } else {
            Err(Error::new(std::io::ErrorKind::NotFound, "no file path set"))
        }
    }

    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        let file_info = FileInfo::from(file_name);
        self.save_to_file(&file_info)?;
        self.file_info = file_info;
        self.dirty = false;
        self.ts_cache = None;
        self.ts_failed = false;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.save_to_file(&self.file_info)?;
        self.dirty = false;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
    pub const fn is_file_loaded(&self) -> bool {
        self.file_info.has_path()
    }
    pub fn height(&self) -> LineIdx {
        self.lines.len()
    }
    pub fn insert_char(&mut self, character: char, at: Location) {
        debug_assert!(at.line_idx <= self.height());
        if at.line_idx == self.height() {
            self.lines.push(Line::from(&character.to_string()));
            self.touch();
        } else if let Some(line) = self.lines.get_mut(at.line_idx) {
            line.insert_char(character, at.grapheme_idx);
            self.touch();
        }
    }
    pub fn delete(&mut self, at: Location) {
        if let Some(line) = self.lines.get(at.line_idx) {
            if at.grapheme_idx >= line.grapheme_count()
                && self.height() > at.line_idx.saturating_add(1)
            {
                let next_line = self.lines.remove(at.line_idx.saturating_add(1));
                #[allow(clippy::indexing_slicing)]
                self.lines[at.line_idx].append(&next_line);
                self.touch();
            } else if at.grapheme_idx < line.grapheme_count() {
                #[allow(clippy::indexing_slicing)]
                self.lines[at.line_idx].delete(at.grapheme_idx);
                self.touch();
            }
        }
    }
    pub fn insert_newline(&mut self, at: Location) {
        if at.line_idx == self.height() {
            self.lines.push(Line::default());
            self.touch();
        } else if let Some(line) = self.lines.get_mut(at.line_idx) {
            let new = line.split(at.grapheme_idx);
            self.lines.insert(at.line_idx.saturating_add(1), new);
            self.touch();
        }
    }

    pub fn substring(&self, line_idx: LineIdx, range: Range<GraphemeIdx>) -> Option<String> {
        self.lines.get(line_idx).map(|line| line.substring(range))
    }

    pub fn delete_range(&mut self, start: Location, end: Location) {
        if start.line_idx == end.line_idx {
            if let Some(line) = self.lines.get_mut(start.line_idx) {
                let mut count = end.grapheme_idx.saturating_sub(start.grapheme_idx);
                while count > 0 {
                    line.delete(start.grapheme_idx);
                    count = count.saturating_sub(1);
                }
                self.touch();
            }
            return;
        }
        if let Some(line) = self.lines.get_mut(start.line_idx) {
            while start.grapheme_idx < line.grapheme_count() {
                line.delete(start.grapheme_idx);
            }
        }
        if let Some(line) = self.lines.get_mut(end.line_idx) {
            let mut remaining = end.grapheme_idx;
            while remaining > 0 {
                line.delete(0);
                remaining = remaining.saturating_sub(1);
            }
        }
        if let Some(end_line) = self.lines.get(end.line_idx).cloned() {
            if let Some(start_line) = self.lines.get_mut(start.line_idx) {
                start_line.append(&end_line);
            }
        }
        let lines_to_remove = end.line_idx.saturating_sub(start.line_idx);
        for _ in 0..lines_to_remove {
            let remove_idx = start.line_idx.saturating_add(1);
            if remove_idx < self.lines.len() {
                self.lines.remove(remove_idx);
            }
        }
        self.touch();
    }

    pub fn delete_leading_tab_or_spaces(&mut self, line_idx: LineIdx) -> GraphemeIdx {
        if let Some(line) = self.lines.get_mut(line_idx) {
            if line.grapheme_at(0) == Some("\t") {
                line.delete(0);
                self.touch();
                1
            } else {
                let mut removed = 0;
                while removed < INDENT.len() && line.grapheme_at(0) == Some(" ") {
                    line.delete(0);
                    removed += 1;
                }
                if removed > 0 {
                    self.touch();
                }
                removed
            }
        } else {
            0
        }
    }
    pub fn location_for_position(&self, line_idx: LineIdx, col: ColIdx) -> Location {
        if self.lines.is_empty() {
            return Location::default();
        }
        let line_idx = line_idx.min(self.lines.len().saturating_sub(1));
        let grapheme_idx = self.lines.get(line_idx).map_or(0, |line| line.grapheme_idx_for_width(col));
        Location { line_idx, grapheme_idx }
    }
}

fn leading_whitespace(line: &Line) -> String {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

#[allow(clippy::integer_division)]
fn indent_level(whitespace: &str) -> usize {
    let tabs = whitespace.matches('\t').count();
    let spaces = whitespace.chars().filter(|c| *c == ' ').count();
    tabs.saturating_add(spaces / INDENT.len())
}
