use super::super::super::{AnnotatedString, Annotation};
use super::tscache::TsCache;
mod history;
mod pairs;
mod encoding;
pub use encoding::Encoding;
use history::History;
use super::FileInfo;
use super::Highlighter;
use super::Line;
use crate::prelude::*;
use std::fs::{self, File, OpenOptions};
use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
mod folding;
mod objects;
use std::collections::{BTreeSet, HashMap};
use super::folds::Fold;

const MAX_TREE_SITTER_LINES: usize = 150_000;
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(PartialEq, Eq)]
enum Atomic {
    Done,
    Unsuitable,
}

pub struct Candidate {
    pub text: String,
    pub kind: &'static str,
}

fn identifiers(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        let word = c.is_alphanumeric() || c == '_';
        match (word, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out
}

fn hard_linked(meta: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.nlink() > 1
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        false
    }
}

#[cfg(unix)]
fn xattrs_unsupported(err: &Error) -> bool {
    err.kind() == ErrorKind::Unsupported || matches!(err.raw_os_error(), Some(95 | 45))
}

#[cfg(unix)]
fn copy_xattrs(from: &Path, to: &Path) -> bool {
    let names = match xattr::list(from) {
        Ok(names) => names,
        Err(err) => return xattrs_unsupported(&err),
    };
    for name in names {
        match xattr::get(from, &name) {
            Ok(Some(value)) => {
                // A label inherited from the directory may already match.
                let same = matches!(xattr::get(to, &name), Ok(Some(v)) if v == value);
                if !same && xattr::set(to, &name, &value).is_err() {
                    return false;
                }
            }
            Ok(None) => {}
            Err(_) => return false,
        }
    }
    true
}

#[cfg(unix)]
fn preserve_identity(original: &fs::Metadata, source: &Path, tmp: &Path, file: &File) -> bool {
    use std::os::unix::fs::{fchown, MetadataExt};
    let already_same = file
        .metadata()
        .is_ok_and(|m| m.uid() == original.uid() && m.gid() == original.gid());
    if !already_same
        && fchown(file, Some(original.uid()), Some(original.gid())).is_err()
    {
        return false;
    }
    copy_xattrs(source, tmp)
}

#[cfg(not(unix))]
fn preserve_identity(_: &fs::Metadata, _: &Path, _: &Path, _: &File) -> bool {
    true
}

fn write_atomic(
    target: &Path,
    original: Option<&fs::Metadata>,
    content: &[u8],
) -> Result<Atomic, Error> {
    let dir = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.display().to_string(),
        _ => ".".to_string(),
    };
    let (tmp, mut file) = match create_temp_file(target, original.is_some()) {
        Ok(created) => created,
        Err(err) if original.is_some() && err.kind() == ErrorKind::PermissionDenied => {
            return Ok(Atomic::Unsuitable);
        }
        Err(err) => return Err(context(&format!("cannot create temp file in '{dir}'"), err)),
    };

    let prepared = file
        .write_all(content)
        .map_err(|err| context("cannot write data", err))
        .and_then(|()| file.sync_all().map_err(|err| context("cannot flush to disk", err)))
        .map(|()| match original {
            Some(meta) => preserve_identity(meta, target, &tmp, &file),
            None => true,
        });
    drop(file);

    let result = prepared.and_then(|preserved| {
        if !preserved {
            return Ok(Atomic::Unsuitable);
        }
        if let Some(meta) = original {
            fs::set_permissions(&tmp, meta.permissions())
                .map_err(|err| context("could not preserve permissions", err))?;
        }
        fs::rename(&tmp, target).map_err(|err| context("cannot replace original file", err))?;
        Ok(Atomic::Done)
    });
    if !matches!(result, Ok(Atomic::Done)) {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_in_place(target: &Path, content: &[u8]) -> Result<(), Error> {
    let backup = fs::read(target).map_err(|err| context("cannot back up original", err))?;
    let mut file = OpenOptions::new()
        .write(true)
        .open(target)
        .map_err(|err| context("cannot open file for writing", err))?;

    let new_len = u64::try_from(content.len()).unwrap_or(u64::MAX);
    let attempt = file
        .write_all(content)
        .and_then(|()| file.set_len(new_len))
        .and_then(|()| file.sync_all());
    let Err(err) = attempt else {
        return Ok(());
    };

    let old_len = u64::try_from(backup.len()).unwrap_or(u64::MAX);
    let restored = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(&backup))
        .and_then(|()| file.set_len(old_len))
        .and_then(|()| file.sync_all());
    let note = if restored.is_ok() {
        "original restored"
    } else {
        "ORIGINAL COULD NOT BE RESTORED"
    };
    Err(Error::new(
        err.kind(),
        format!(
            "cannot write data in place ({note}): {}",
            crate::editor::errors::reason(&err)
        ),
    ))
}

fn create_temp_file(target: &Path, target_exists: bool) -> Result<(PathBuf, File), Error> {
    for _ in 0..16 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = target.with_file_name(format!(
            ".vmacs-{}-{seq}-{nanos:x}.tmp",
            std::process::id()
        ));

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(if target_exists { 0o600 } else { 0o666 });
        }
        #[cfg(not(unix))]
        let _ = target_exists;

        match options.open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Err(Error::new(
        ErrorKind::AlreadyExists,
        "could not create a unique temp file",
    ))
}

#[derive(Default)]
pub struct Buffer {
    lines: Vec<Line>,
    file_info: FileInfo,
    dirty: bool,
    revision: u64,
    ts_cache: Option<TsCache>,
    ts_failed: bool,
    history: History,
    line_ending: LineEnding,
    encoding: Encoding,
    read_only: bool,
    no_final_newline: bool,
    mixed_eol: bool,
    folds: Vec<Fold>,
    folds_rev: Option<u64>,
    closed: BTreeSet<usize>,
    disk_mtime: Option<std::time::SystemTime>,
    fold_anim: Option<folding::FoldAnim>,
    animate_folds: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum LineEnding {
    #[default]
    Lf,
    Crlf,
    Cr,
}

impl LineEnding {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

fn context(what: &str, err: Error) -> Error {
    Error::new(err.kind(), format!("{what}: {}", crate::editor::errors::reason(&err)))
}

impl Buffer {
    fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn refresh_language(&mut self) {
        self.file_info.refresh_language();
        self.ts_cache = None;
        self.ts_failed = false;
        self.folds_rev = None;
        self.revision = self.revision.wrapping_add(1);
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub const fn get_file_info(&self) -> &FileInfo {
        &self.file_info
    }

    pub fn refresh_syntax(&mut self) {
        self.refresh_tree();
        self.refresh_folds();
    }

    pub fn refresh_tree(&mut self) {
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

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn word_before(&self, at: Location) -> Option<(String, GraphemeIdx)> {
        let line = self.lines.get(at.line_idx)?;
        let is_word = |g: &str| g.chars().all(|c| c.is_alphanumeric() || c == '_');
        let end = at.grapheme_idx.min(line.grapheme_count());
        let mut start = end;
        while start > 0 && line.grapheme_at(start.saturating_sub(1)).is_some_and(is_word) {
            start = start.saturating_sub(1);
        }
        let word = line.substring(start..end);
        (!word.is_empty() && !word.starts_with(|c: char| c.is_ascii_digit())).then_some((word, start))
    }

    pub fn disk_changed(&self) -> bool {
        let Some(known) = self.disk_mtime else { return false };
        self.file_info
            .get_path()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .is_some_and(|now| now != known)
    }

    pub fn completions(
        &self,
        at: Location,
        prefix: &str,
        start: GraphemeIdx,
        limit: usize,
    ) -> Vec<Candidate> {
        const SPAN: usize = 3000;
        let cur_byte = self
            .lines
            .get(at.line_idx)
            .map(|line| line.byte_range_for_grapheme_range(start..start).start);
        let lo = at.line_idx.saturating_sub(SPAN);
        let hi = at.line_idx.saturating_add(SPAN).min(self.lines.len());

        let mut found: HashMap<String, usize> = HashMap::new();
        for idx in lo..hi {
            let Some(line) = self.lines.get(idx) else { break };
            let text: &str = line;
            if !text.contains(prefix) {
                continue;
            }
            let dist = idx.abs_diff(at.line_idx);
            for (s, word) in identifiers(text) {
                if idx == at.line_idx && Some(s) == cur_byte {
                    continue; // the word being typed
                }
                if word.len() > prefix.len()
                    && word.starts_with(prefix)
                    && !word.starts_with(|c: char| c.is_ascii_digit())
                {
                    let slot = found.entry(word.to_string()).or_insert(dist);
                    *slot = (*slot).min(dist);
                }
            }
        }
        let mut ranked: Vec<(usize, String)> = found.into_iter().map(|(w, d)| (d, w)).collect();
        ranked.sort();
        let bufs: Vec<Candidate> = ranked
            .into_iter()
            .map(|(_, text)| Candidate { text, kind: "buf" })
            .collect();

        let mut extra: Vec<Candidate> = Vec::new();
        if let Some(lang) = self.file_info.get_language() {
            let ci = lang.case_insensitive;
            let p = if ci { prefix.to_ascii_lowercase() } else { prefix.to_string() };
            let sets: [(&Vec<String>, &'static str); 3] = [
                (&lang.config.keywords, "kw"),
                (&lang.config.types, "type"),
                (&lang.config.known_values, "val"),
            ];
            for (words, kind) in sets {
                for w in words {
                    let hit = if ci { w.to_ascii_lowercase().starts_with(&p) } else { w.starts_with(&p) };
                    if hit
                        && w.len() > prefix.len()
                        && !bufs.iter().any(|c| c.text == *w)
                        && !extra.iter().any(|c| c.text == *w)
                    {
                        extra.push(Candidate { text: w.clone(), kind });
                    }
                }
            }
            extra.sort_by(|a, b| a.text.cmp(&b.text));
        }

        // Keep a few slots for language words so they aren't crowded out.
        let keep = limit.saturating_sub(extra.len().min(limit / 3));
        let mut out: Vec<Candidate> = bufs.into_iter().take(keep).collect();
        out.extend(extra);
        out.truncate(limit);
        out
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
            .any(|trigger| before.ends_with(trigger.as_str()));

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
        self.edit_line(line_idx, |line| {
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
            (old, new)
        })
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
        self.op_remove_line(line_idx);
    }

    pub fn grapheme_at(&self, at: Location) -> Option<String> {
        self.lines
            .get(at.line_idx)
            .and_then(|line| line.grapheme_at(at.grapheme_idx))
            .map(str::to_string)
    }

    pub fn delete_to_line_end(&mut self, at: Location) {
        self.edit_line(at.line_idx, |line| {
            let count = line.grapheme_count();
            for _ in at.grapheme_idx..count {
                line.delete(at.grapheme_idx);
            }
        });
    }

    pub fn insert_line(&mut self, idx: LineIdx, content: &str) {
        self.op_insert_line(idx, Line::from(content));
    }

    fn class_at(&self, line_idx: LineIdx, g: GraphemeIdx, big: bool) -> u8 {
        self.lines
            .get(line_idx)
            .and_then(|line| line.grapheme_at(g))
            .map_or(0, |s| word_class(s, big))
    }

    fn next_start(&self, from: Location, big: bool) -> Location {
        let (mut l, mut g) = (from.line_idx, from.grapheme_idx);
        let Some(line) = self.lines.get(l) else {
            return from;
        };
        let cls = self.class_at(l, g, big);
        if cls != 0 {
            while g < line.grapheme_count() && self.class_at(l, g, big) == cls {
                g = g.saturating_add(1);
            }
        }
        loop {
            let count = self.grapheme_count(l);
            while g < count && self.class_at(l, g, big) == 0 {
                g = g.saturating_add(1);
            }
            if g < count {
                return Location { line_idx: l, grapheme_idx: g };
            }
            if l.saturating_add(1) >= self.height() {
                return Location { line_idx: l, grapheme_idx: count };
            }
            l = l.saturating_add(1);
            g = 0;
            if self.grapheme_count(l) == 0 {
                return Location { line_idx: l, grapheme_idx: 0 };
            }
        }
    }

    fn prev_start(&self, from: Location, big: bool) -> Location {
        let (mut l, mut g) = (from.line_idx, from.grapheme_idx);
        loop {
            if g == 0 {
                if l == 0 {
                    return Location { line_idx: 0, grapheme_idx: 0 };
                }
                l = l.saturating_sub(1);
                g = self.grapheme_count(l);
                if g == 0 {
                    return Location { line_idx: l, grapheme_idx: 0 };
                }
                continue;
            }
            if self.class_at(l, g.saturating_sub(1), big) == 0 {
                g = g.saturating_sub(1);
                continue;
            }
            break;
        }
        let cls = self.class_at(l, g.saturating_sub(1), big);
        while g > 0 && self.class_at(l, g.saturating_sub(1), big) == cls {
            g = g.saturating_sub(1);
        }
        Location { line_idx: l, grapheme_idx: g }
    }

    fn word_end(&self, from: Location, big: bool) -> Location {
        let Some(last_line) = self.lines.len().checked_sub(1) else {
            return Location::default();
        };
        let mut l = from.line_idx.min(last_line);
        let mut g = from.grapheme_idx.saturating_add(1);
        loop {
            let count = self.grapheme_count(l);
            if g >= count {
                if l >= last_line {
                    return Location { line_idx: l, grapheme_idx: count.saturating_sub(1) };
                }
                l = l.saturating_add(1);
                g = 0;
                continue;
            }
            if self.class_at(l, g, big) == 0 {
                g = g.saturating_add(1);
            } else {
                break;
            }
        }
        let count = self.grapheme_count(l);
        let cls = self.class_at(l, g, big);
        while g.saturating_add(1) < count && self.class_at(l, g.saturating_add(1), big) == cls {
            g = g.saturating_add(1);
        }
        Location { line_idx: l, grapheme_idx: g }
    }

    pub fn find_next_word_start(&self, from: Location) -> Location { self.next_start(from, false) }
    pub fn find_prev_word_start(&self, from: Location) -> Location { self.prev_start(from, false) }
    pub fn find_word_end(&self, from: Location) -> Location { self.word_end(from, false) }
    pub fn find_next_bigword_start(&self, from: Location) -> Location { self.next_start(from, true) }
    pub fn find_prev_bigword_start(&self, from: Location) -> Location { self.prev_start(from, true) }
    pub fn find_bigword_end(&self, from: Location) -> Location { self.word_end(from, true) }

    pub fn first_non_blank(&self, line_idx: LineIdx) -> GraphemeIdx {
        let Some(line) = self.lines.get(line_idx) else {
            return 0;
        };
        let count = line.grapheme_count();
        (0..count)
            .find(|&i| line.grapheme_at(i).is_some_and(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| count.saturating_sub(1))
    }

    pub fn first_non_blank_or_end(&self, line_idx: LineIdx) -> GraphemeIdx {
        let Some(line) = self.lines.get(line_idx) else {
            return 0;
        };
        let count = line.grapheme_count();
        (0..count)
            .find(|&i| line.grapheme_at(i).is_some_and(|s| !s.trim().is_empty()))
            .unwrap_or(count)
    }

    pub fn indent_end(&self, line_idx: LineIdx) -> GraphemeIdx {
        let Some(line) = self.lines.get(line_idx) else {
            return 0;
        };
        let count = line.grapheme_count();
        let indent = (0..count)
            .take_while(|&i| matches!(line.grapheme_at(i), Some(" " | "\t")))
            .count();
        if indent == count { 0 } else { indent }
    }

    pub fn guide_indent(&self, line_idx: LineIdx) -> usize {
        if !self.is_blank_line(line_idx) {
            return self.indent_end(line_idx);
        }
        let mut above = 0;
        for i in (line_idx.saturating_sub(50)..line_idx).rev() {
            if !self.is_blank_line(i) {
                above = self.indent_end(i);
                break;
            }
        }
        let mut below = 0;
        let end = line_idx.saturating_add(51).min(self.lines.len());
        for i in line_idx.saturating_add(1)..end {
            if !self.is_blank_line(i) {
                below = self.indent_end(i);
                break;
            }
        }
        above.max(below)
    }

    pub fn indent_unit(&self) -> usize {
        let (mut indented, mut twos) = (0usize, 0usize);
        for line in self.lines.iter().take(500) {
            if line.trim().is_empty() {
                continue;
            }
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            if spaces > 0 {
                indented = indented.saturating_add(1);
                if spaces % 4 == 2 {
                    twos = twos.saturating_add(1);
                }
            }
        }
        if twos.saturating_mul(5) > indented { 2 } else { 4 }
    }

    pub fn last_non_blank(&self, line_idx: LineIdx) -> GraphemeIdx {
        let Some(line) = self.lines.get(line_idx) else {
            return 0;
        };
        (0..line.grapheme_count())
            .rev()
            .find(|&i| line.grapheme_at(i).is_some_and(|s| !s.trim().is_empty()))
            .unwrap_or(0)
    }

    pub fn find_char_in_line(
        &self,
        at: Location,
        ch: char,
        forward: bool,
        till: bool,
    ) -> Option<Location> {
        let line = self.lines.get(at.line_idx)?;
        let is_target = |i: usize| {
            line.grapheme_at(i).is_some_and(|g| {
                let mut chars = g.chars();
                chars.next() == Some(ch) && chars.next().is_none()
            })
        };
        let grapheme_idx = if forward {
            let found = (at.grapheme_idx.saturating_add(1)..line.grapheme_count())
                .find(|&i| is_target(i))?;
            if till { found.saturating_sub(1) } else { found }
        } else {
            let found = (0..at.grapheme_idx.min(line.grapheme_count()))
                .rev()
                .find(|&i| is_target(i))?;
            if till { found.saturating_add(1) } else { found }
        };
        Some(Location { line_idx: at.line_idx, grapheme_idx })
    }

    fn is_blank_line(&self, idx: LineIdx) -> bool {
        self.lines.get(idx).is_some_and(|line| line.trim().is_empty())
    }

    pub fn find_next_paragraph(&self, from: Location) -> Location {
        let Some(last) = self.lines.len().checked_sub(1) else {
            return Location::default();
        };
        let mut idx = from.line_idx.min(last);
        while idx < last && self.is_blank_line(idx) {
            idx = idx.saturating_add(1);
        }
        while idx < last && !self.is_blank_line(idx) {
            idx = idx.saturating_add(1);
        }
        let grapheme_idx = if self.is_blank_line(idx) { 0 } else { self.grapheme_count(idx) };
        Location { line_idx: idx, grapheme_idx }
    }

    pub fn find_prev_paragraph(&self, from: Location) -> Location {
        let Some(last) = self.lines.len().checked_sub(1) else {
            return Location::default();
        };
        let mut idx = from.line_idx.min(last);
        while idx > 0 && self.is_blank_line(idx) {
            idx = idx.saturating_sub(1);
        }
        while idx > 0 && !self.is_blank_line(idx) {
            idx = idx.saturating_sub(1);
        }
        Location { line_idx: idx, grapheme_idx: 0 }
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
        Self::load_with(file_name, None)
    }


    pub fn load_with(file_name: &str, forced: Option<Encoding>) -> Result<Self, Error> {
        let bytes = match fs::read(file_name) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(err),
        };
        let (contents, encoding) = match forced {
            Some(encoding) => (encoding.decode_as(&bytes)?, encoding),
            None => Encoding::detect(&bytes)?,
        };
        let crlf = contents.matches("\r\n").count();
        let lf_only = contents.matches('\n').count().saturating_sub(crlf);
        let cr_only = contents.matches('\r').count().saturating_sub(crlf);
        let cr_file = crlf == 0 && lf_only == 0 && cr_only > 0;
        let mixed_eol = crlf > 0 && lf_only > 0;
        let line_ending = if cr_file {
            LineEnding::Cr
        } else if crlf > lf_only {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        };
        let final_eol = if cr_file { '\r' } else { '\n' };
        let no_final_newline = !contents.is_empty() && !contents.ends_with(final_eol);
        let lines: Vec<Line> = if cr_file {
            let mut parts: Vec<&str> = contents.split('\r').collect();
            if contents.ends_with('\r') {
                parts.pop();
            }
            parts.into_iter().map(Line::from).collect()
        } else {
            contents.lines().map(Line::from).collect()
        };

        let read_only = fs::metadata(file_name).is_ok_and(|meta| meta.is_file())
            && OpenOptions::new().write(true).open(file_name).is_err();
        
        let mut file_info = FileInfo::from(file_name);
        if let Some(first) = lines.first() {
            file_info.detect_shebang(first);
        }

        Ok(Self {
            lines,
            file_info,
            line_ending,
            encoding,
            read_only,
            no_final_newline,
            mixed_eol,
            disk_mtime: fs::metadata(file_name).and_then(|m| m.modified()).ok(),
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

    fn save_to_file(&self, file_info: &FileInfo, check_disk: bool) -> Result<(), Error> {
        let Some(path) = file_info.get_path() else {
            return Err(Error::new(ErrorKind::NotFound, "no file name set (use :w <name>)"));
        };
        let eol = self.line_ending.as_str();
        let mut content = String::new();
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            content.push_str(line);
            if i != last || !self.no_final_newline {
                content.push_str(eol);
            }
        }

        let target = match fs::canonicalize(path) {
            Ok(resolved) => resolved,
            Err(err) if err.kind() == ErrorKind::NotFound => path.to_path_buf(),
            Err(err) => return Err(context("cannot resolve path", err)),
        };
        let original_meta = match fs::metadata(&target) {
            Ok(meta) => Some(meta),
            Err(err) if err.kind() == ErrorKind::NotFound => None,
            Err(err) => return Err(context("cannot read file metadata", err)),
        };
        if let Some(meta) = &original_meta {
            if meta.is_dir() {
                return Err(Error::new(ErrorKind::InvalidInput, "path is a directory"));
            }
            if meta.permissions().readonly() {
                return Err(Error::new(ErrorKind::PermissionDenied, "file is read-only"));
            }
            if check_disk && self.disk_mtime.is_some_and(|known| meta.modified().is_ok_and(|now| now != known)) {
                return Err(Error::new(
                    ErrorKind::Other,
                    "file changed on disk since it was loaded (:w! overwrites)",
                ));
            }
        }

        let bytes = self
            .encoding
            .encode(&content)
            .map_err(|message| Error::new(ErrorKind::InvalidData, message))?;
        if !original_meta.as_ref().is_some_and(hard_linked)
            && write_atomic(&target, original_meta.as_ref(), &bytes)? == Atomic::Done
        {
            return Ok(());
        }
        write_in_place(&target, &bytes)
    }

    pub const fn line_ending_label(&self) -> &'static str {
        match (self.line_ending, self.mixed_eol) {
            (LineEnding::Lf, false) => "LF",
            (LineEnding::Crlf, false) => "CRLF",
            (LineEnding::Cr, false) => "CR",
            (LineEnding::Lf, true) => "LF*",
            (LineEnding::Crlf, true) => "CRLF*",
            (LineEnding::Cr, true) => "CR*",
        }
    }

    pub const fn encoding_label(&self) -> &'static str {
        self.encoding.label()
    }

    pub fn set_encoding(&mut self, encoding: Encoding) {
        if self.encoding != encoding {
            self.encoding = encoding;
            self.touch();
        }
    }

    pub fn set_line_ending_named(&mut self, name: &str) -> Option<&'static str> {
        let ending = match name.trim().to_lowercase().as_str() {
            "lf" | "unix" => LineEnding::Lf,
            "crlf" | "dos" | "windows" => LineEnding::Crlf,
            "cr" | "mac" => LineEnding::Cr,
            _ => return None,
        };
        if self.line_ending != ending || self.mixed_eol {
            self.line_ending = ending;
            self.mixed_eol = false;
            self.touch();
        }
        Some(self.line_ending_label())
    }

    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, on: bool) {
        self.read_only = on;
    }

    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        let file_info = FileInfo::from(file_name);
        self.save_to_file(&file_info, false)?;
        self.file_info = file_info;
        self.read_only = false;
        self.dirty = false;
        self.mark_saved();
        self.refresh_mtime();
        self.ts_cache = None;
        self.ts_failed = false;
        self.folds_rev = None;
        Ok(())
    }

    pub fn save_with(&mut self, force: bool) -> Result<(), Error> {
        self.save_to_file(&self.file_info, !force)?;
        self.dirty = false;
        self.mark_saved();
        self.refresh_mtime();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn save(&mut self) -> Result<(), Error> {
        self.save_with(false)
    }

    fn refresh_mtime(&mut self) {
        self.disk_mtime = self
            .file_info
            .get_path()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
    }

    pub fn line_comment(&self) -> Option<String> {
        self.file_info
            .get_language()
            .and_then(|lang| lang.config.line_comment.clone())
    }

    pub fn indent_style(&self) -> &'static str {
        let (mut tabs, mut spaces) = (0usize, 0usize);
        for line in self.lines.iter().take(500) {
            match line.chars().next() {
                Some('\t') => tabs = tabs.saturating_add(1),
                Some(' ') => spaces = spaces.saturating_add(1),
                _ => {}
            }
        }
        if tabs > spaces { "Tabs" } else { "Spaces: 4" }
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
            self.op_insert_line(at.line_idx, Line::from(&character.to_string()));
        } else {
            self.edit_line(at.line_idx, |line| {
                line.insert_char(character, at.grapheme_idx);
            });
        }
    }
    pub fn delete(&mut self, at: Location) {
        let Some(count) = self.lines.get(at.line_idx).map(Line::grapheme_count) else {
            return;
        };
        if at.grapheme_idx >= count && self.height() > at.line_idx.saturating_add(1) {
            // Join with the next line.
            if let Some(next_line) = self.op_remove_line(at.line_idx.saturating_add(1)) {
                self.edit_line(at.line_idx, |line| line.append(&next_line));
            }
        } else if at.grapheme_idx < count {
            self.edit_line(at.line_idx, |line| line.delete(at.grapheme_idx));
        }
    }
    pub fn insert_newline(&mut self, at: Location) {
        if at.line_idx == self.height() {
            self.op_insert_line(at.line_idx, Line::default());
        } else if let Some(new) = self.edit_line(at.line_idx, |line| line.split(at.grapheme_idx)) {
            self.op_insert_line(at.line_idx.saturating_add(1), new);
        }
    }

    pub fn substring(&self, line_idx: LineIdx, range: Range<GraphemeIdx>) -> Option<String> {
        self.lines.get(line_idx).map(|line| line.substring(range))
    }

    pub fn delete_range(&mut self, start: Location, end: Location) {
        let (Some(first), Some(last)) = (
            self.lines.get(start.line_idx),
            self.lines.get(end.line_idx),
        ) else {
            return;
        };
        let head = first.substring(0..start.grapheme_idx);
        let tail = last.substring(end.grapheme_idx..last.grapheme_count());
        let merged = Line::from(&format!("{head}{tail}"));
        self.op_set_line(start.line_idx, merged);
        for _ in start.line_idx..end.line_idx {
            self.op_remove_line(start.line_idx.saturating_add(1));
        }
    }

    pub fn delete_leading_tab_or_spaces(&mut self, line_idx: LineIdx) -> GraphemeIdx {
        self.edit_line(line_idx, |line| {
            if line.grapheme_at(0) == Some("\t") {
                line.delete(0);
                1
            } else {
                let mut removed = 0;
                while removed < INDENT.len() && line.grapheme_at(0) == Some(" ") {
                    line.delete(0);
                    removed += 1;
                }
                removed
            }
        })
        .unwrap_or(0)
    }
    pub fn location_for_position(&self, line_idx: LineIdx, col: ColIdx) -> Location {
        if self.lines.is_empty() {
            return Location::default();
        }
        let line_idx = line_idx.min(self.lines.len().saturating_sub(1));
        let grapheme_idx = self.lines.get(line_idx).map_or(0, |line| line.grapheme_idx_for_width(col));
        Location { line_idx, grapheme_idx }
    }
    pub fn clamp_location(&self, at: Location) -> Location {
        match self.lines.len().checked_sub(1) {
            Some(last) if at.line_idx > last => Location {
                line_idx: last,
                grapheme_idx: self.grapheme_count(last),
            },
            _ => at,
        }
    }

    pub fn insert_str(&mut self, at: Location, text: &str) -> Location {
        let mut segments = text.split('\n');
        let Some(first) = segments.next() else {
            return at;
        };
        let rest: Vec<&str> = segments.collect();

        let line_idx = at.line_idx.min(self.height());
        if line_idx == self.height() {
            self.op_insert_line(line_idx, Line::default());
        }
        let grapheme_idx = at.grapheme_idx.min(self.grapheme_count(line_idx));

        let first_line = Line::from(first);
        let first_width = first_line.grapheme_count();

        let tail = self
            .edit_line(line_idx, |line| {
                let tail = line.split(grapheme_idx);
                line.append(&first_line);
                tail
            })
            .unwrap_or_default();

        let Some((last, middle)) = rest.split_last() else {
            // Single-line paste: put the tail back after the pasted text.
            self.edit_line(line_idx, |line| line.append(&tail));
            return Location {
                line_idx,
                grapheme_idx: grapheme_idx.saturating_add(first_width),
            };
        };

        let mut next_idx = line_idx.saturating_add(1);
        for segment in middle {
            self.op_insert_line(next_idx, Line::from(segment));
            next_idx = next_idx.saturating_add(1);
        }
        let mut last_line = Line::from(last);
        let last_width = last_line.grapheme_count();
        last_line.append(&tail);
        self.op_insert_line(next_idx, last_line);

        Location {
            line_idx: next_idx,
            grapheme_idx: last_width,
        }
    }

    pub fn replace_line_text(&mut self, idx: LineIdx, text: &str) {
        self.op_set_line(idx, Line::from(text));
    }

    pub fn search_forward_nowrap(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        for line_idx in from.line_idx..self.lines.len() {
            let line = self.lines.get(line_idx)?;
            let start = if line_idx == from.line_idx { from.grapheme_idx } else { 0 };
            if start > line.grapheme_count() {
                continue;
            }
            if let Some(grapheme_idx) = line.search_forward(query, start) {
                return Some(Location { grapheme_idx, line_idx });
            }
        }
        None
    }

    pub fn replace_at(&mut self, at: Location, pattern: &str, text: &str) -> GraphemeIdx {
        let delete = Line::from(pattern).grapheme_count();
        self.edit_line(at.line_idx, |line| {
            for _ in 0..delete {
                line.delete(at.grapheme_idx);
            }
            let tail = line.split(at.grapheme_idx);
            let inserted = Line::from(text);
            let count = inserted.grapheme_count();
            line.append(&inserted);
            line.append(&tail);
            count
        })
        .unwrap_or(0)
    }
}

fn leading_whitespace(line: &Line) -> String {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn word_class(g: &str, big: bool) -> u8 {
    if g.chars().all(char::is_whitespace) {
        0
    } else if big || g.chars().all(|c| c.is_alphanumeric() || c == '_') {
        1
    } else {
        2
    }
}

#[allow(clippy::integer_division)]
fn indent_level(whitespace: &str) -> usize {
    let tabs = whitespace.matches('\t').count();
    let spaces = whitespace.chars().filter(|c| *c == ' ').count();
    tabs.saturating_add(spaces / INDENT.len())
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn loc(line_idx: usize, grapheme_idx: usize) -> Location {
        Location { line_idx, grapheme_idx }
    }

    fn open(dir: &TempDir, name: &str, bytes: &[u8]) -> (Buffer, PathBuf) {
        let path = dir.write(name, bytes);
        let buffer = Buffer::load(path.to_str().unwrap()).unwrap();
        (buffer, path)
    }

    fn roundtrip(bytes: &[u8]) -> Vec<u8> {
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", bytes);
        buffer.save().unwrap();
        fs::read(path).unwrap()
    }

    // ---- load / save fidelity -------------------------------------------

    #[test]
    fn save_preserves_bom_final_newline_and_crlf() {
        let cases: &[&[u8]] = &[
            b"abc\n",
            b"abc",
            b"",
            b"\n",
            b"\xef\xbb\xbfabc\n",
            b"\xef\xbb\xbf",
            b"a\r\nb\r\n",
            b"a\r\nb",
            b"a\rb\n",
            b"\n\n\n",
        ];
        for case in cases {
            assert_eq!(roundtrip(case), *case, "case {case:?}");
        }
    }

    #[test]
    fn bom_is_not_part_of_the_text() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"\xef\xbb\xbfabc\n");
        assert_eq!(buffer.line_content(0).as_deref(), Some("abc"));
    }

    #[test]
    fn mixed_line_endings_are_flagged_and_normalized() {
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", b"a\r\nb\r\nc\n");
        assert_eq!(buffer.line_ending_label(), "CRLF*");
        buffer.save().unwrap();
        assert_eq!(fs::read(path).unwrap(), b"a\r\nb\r\nc\r\n");
    }

    #[test]
    fn line_ending_labels() {
        let dir = TempDir::new();
        assert_eq!(open(&dir, "a.txt", b"a\nb\n").0.line_ending_label(), "LF");
        assert_eq!(open(&dir, "b.txt", b"a\r\nb\r\n").0.line_ending_label(), "CRLF");
    }

    #[test]
    fn binary_files_are_refused() {
        let dir = TempDir::new();
        let path = dir.write("bin", b"abc\x00def");
        let err = Buffer::load(path.to_str().unwrap()).err().expect("should fail");
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn legacy_and_utf16_files_roundtrip() {
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", b"caf\xe9\n");
        assert_eq!(buffer.encoding_label(), "CP1252");
        assert_eq!(buffer.line_content(0).as_deref(), Some("café"));
        buffer.save().unwrap();
        assert_eq!(fs::read(path).unwrap(), b"caf\xe9\n");

        assert_eq!(roundtrip(b"\xff\xfea\x00\n\x00"), b"\xff\xfea\x00\n\x00");
        assert_eq!(roundtrip(b"a\rb\r"), b"a\rb\r");
        assert_eq!(roundtrip(b"a\rb"), b"a\rb");
        let (cr, _) = open(&dir, "g.txt", b"a\rb\r");
        assert_eq!((cr.height(), cr.line_ending_label()), (2, "CR"));
    }

    #[test]
    fn unmappable_characters_refuse_to_save() {
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", b"a\n");
        buffer.set_encoding(Encoding::Latin1);
        buffer.replace_line_text(0, "€");
        assert_eq!(buffer.save().unwrap_err().kind(), ErrorKind::InvalidData);
        assert_eq!(fs::read(path).unwrap(), b"a\n");
    }

    #[test]
    fn missing_file_is_an_empty_buffer() {
        let dir = TempDir::new();
        let path = dir.path().join("new.txt");
        let buffer = Buffer::load(path.to_str().unwrap()).unwrap();
        assert!(buffer.is_empty());
        assert!(buffer.is_file_loaded());
    }

    #[test]
    fn save_as_creates_a_new_file() {
        let dir = TempDir::new();
        let path = dir.path().join("new.txt");
        let mut buffer = Buffer::default();
        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('i', loc(0, 1));
        buffer.save_as(path.to_str().unwrap()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hi\n");
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn save_errors_are_reported_and_leave_the_file_alone() {
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", b"keep\n");
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).unwrap();
        buffer.replace_line_text(0, "changed");
        let err = buffer.save().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&path).unwrap(), b"keep\n");

        let mut other = Buffer::default();
        assert_eq!(
            other.save_as(dir.path().to_str().unwrap()).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn save_leaves_no_temp_files() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"a\n");
        buffer.replace_line_text(0, "b");
        buffer.save().unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["f.txt".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn save_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let (mut buffer, path) = open(&dir, "f.txt", b"a\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        buffer.replace_line_text(0, "b");
        buffer.save().unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn save_keeps_hard_links() {
        use std::os::unix::fs::MetadataExt;
        let dir = TempDir::new();
        let (mut buffer, a) = open(&dir, "a.txt", b"old\n");
        let b = dir.path().join("b.txt");
        fs::hard_link(&a, &b).unwrap();
        buffer.replace_line_text(0, "new");
        buffer.save().unwrap();
        assert_eq!(fs::read(&b).unwrap(), b"new\n");
        let (ma, mb) = (fs::metadata(&a).unwrap(), fs::metadata(&b).unwrap());
        assert_eq!(ma.ino(), mb.ino());
        assert_eq!(ma.nlink(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn save_through_a_symlink_updates_the_target() {
        let dir = TempDir::new();
        let real = dir.write("real.txt", b"a\n");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut buffer = Buffer::load(link.to_str().unwrap()).unwrap();
        buffer.replace_line_text(0, "b");
        buffer.save().unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&real).unwrap(), b"b\n");
    }

    // ---- editing --------------------------------------------------------

    #[test]
    fn insert_str_splits_lines() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"abc\n");
        let end = buffer.insert_str(loc(0, 1), "X\nY");
        assert_eq!(buffer.line_content(0).as_deref(), Some("aX"));
        assert_eq!(buffer.line_content(1).as_deref(), Some("Ybc"));
        assert_eq!((end.line_idx, end.grapheme_idx), (1, 1));
    }

    #[test]
    fn delete_range_across_lines() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"abc\ndef\n");
        buffer.delete_range(loc(0, 1), loc(1, 1));
        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.line_content(0).as_deref(), Some("aef"));
    }

    #[test]
    fn delete_at_line_end_joins_lines() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"ab\ncd\n");
        buffer.delete(loc(0, 2));
        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.line_content(0).as_deref(), Some("abcd"));
    }

    #[test]
    fn newline_splits_the_line() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"abc\n");
        buffer.insert_newline(loc(0, 1));
        assert_eq!(buffer.line_content(0).as_deref(), Some("a"));
        assert_eq!(buffer.line_content(1).as_deref(), Some("bc"));
    }

    #[test]
    fn guide_indent_and_unit() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"a {\n  b {\n\n    c\n  }\n}\n");
        assert_eq!(buffer.guide_indent(1), 2);
        assert_eq!(buffer.guide_indent(2), 4);
        assert_eq!(buffer.indent_unit(), 2);
        let (four, _) = open(&dir, "g.txt", b"a {\n    b\n    c\n}\n");
        assert_eq!(four.indent_unit(), 4);
    }

    #[test]
    fn replace_at_swaps_text() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"foo bar\n");
        let inserted = buffer.replace_at(loc(0, 4), "bar", "quux");
        assert_eq!(inserted, 4);
        assert_eq!(buffer.line_content(0).as_deref(), Some("foo quux"));
    }

    #[test]
    fn word_end_and_blank_motions() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"foo bar\nbaz\n  ab  \n");
        let at = |l: Location| (l.line_idx, l.grapheme_idx);
        assert_eq!(at(buffer.find_word_end(loc(0, 0))), (0, 2));
        assert_eq!(at(buffer.find_word_end(loc(0, 2))), (0, 6));
        assert_eq!(at(buffer.find_word_end(loc(0, 6))), (1, 2));
        assert_eq!(buffer.first_non_blank(2), 2);
        assert_eq!(buffer.last_non_blank(2), 3);
    }

    #[test]
    fn find_char_motions() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"abc,d\n");
        let col = |l: Option<Location>| l.map(|l| l.grapheme_idx);
        assert_eq!(col(buffer.find_char_in_line(loc(0, 0), ',', true, false)), Some(3));
        assert_eq!(col(buffer.find_char_in_line(loc(0, 0), ',', true, true)), Some(2));
        assert_eq!(col(buffer.find_char_in_line(loc(0, 4), ',', false, false)), Some(3));
        assert_eq!(col(buffer.find_char_in_line(loc(0, 4), ',', false, true)), Some(4));
        assert_eq!(col(buffer.find_char_in_line(loc(0, 0), 'z', true, false)), None);
    }

    #[test]
    fn word_motions() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"foo bar\nbaz\n");
        let next = buffer.find_next_word_start(loc(0, 0));
        assert_eq!((next.line_idx, next.grapheme_idx), (0, 4));
        let next = buffer.find_next_word_start(loc(0, 4));
        assert_eq!((next.line_idx, next.grapheme_idx), (1, 0));
        let prev = buffer.find_prev_word_start(loc(0, 7));
        assert_eq!((prev.line_idx, prev.grapheme_idx), (0, 4));
    }

    #[test]
    fn search_wraps_around() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"abc\nxbz\n");
        let hit = buffer.search_forward("b", loc(0, 0)).unwrap();
        assert_eq!((hit.line_idx, hit.grapheme_idx), (0, 1));
        let hit = buffer.search_forward("b", loc(0, 2)).unwrap();
        assert_eq!((hit.line_idx, hit.grapheme_idx), (1, 1));
        let hit = buffer.search_forward("b", loc(1, 2)).unwrap();
        assert_eq!((hit.line_idx, hit.grapheme_idx), (0, 1));
        let hit = buffer.search_backward("b", loc(1, 0)).unwrap();
        assert_eq!((hit.line_idx, hit.grapheme_idx), (0, 1));
        assert!(buffer.search_forward("nope", loc(0, 0)).is_none());
        assert!(buffer.search_forward("", loc(0, 0)).is_none());
        assert!(buffer.search_forward_nowrap("b", loc(1, 2)).is_none());
    }

    // ---- indentation ------------------------------------------------------

    #[test]
    fn indent_helpers() {
        assert_eq!(indent_level("\t"), 1);
        assert_eq!(indent_level("        "), 2);
        assert_eq!(indent_level("  "), 0);
        assert_eq!(leading_whitespace(&Line::from("\t  x")), "\t  ");
    }

    #[test]
    fn newline_indent_follows_triggers() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.cs", b"if (x) {\n    foo();\n");
        assert!(buffer.autoindent_enabled());
        assert_eq!(buffer.newline_indent_level(loc(0, 8)), 1);
        assert_eq!(buffer.newline_indent_level(loc(1, 10)), 1);
    }

    #[test]
    fn plain_text_has_no_autoindent() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"if (x) {\n");
        assert!(!buffer.autoindent_enabled());
        assert_eq!(buffer.newline_indent_level(loc(0, 8)), 0);
    }

    #[test]
    fn indent_end_ignores_blank_lines() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"  \t x\n    \nabc\n");
        assert_eq!(buffer.indent_end(0), 4);
        assert_eq!(buffer.indent_end(1), 0);
        assert_eq!(buffer.indent_end(2), 0);
    }

    #[test]
    fn closer_dedents_to_its_opener() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.cs", b"    if (x) {\n        y();\n    }\n");
        assert_eq!(buffer.matching_open_indent(loc(2, 4), '}').as_deref(), Some("    "));
        assert!(buffer.matching_open_indent(loc(2, 4), 'x').is_none());
    }

    #[test]
    fn set_leading_whitespace_reports_the_change() {
        let dir = TempDir::new();
        let (mut buffer, _) = open(&dir, "f.txt", b"  x\n");
        assert_eq!(buffer.set_leading_whitespace(0, "\t"), Some((2, 1)));
        assert_eq!(buffer.line_content(0).as_deref(), Some("\tx"));
    }

    #[test]
    fn paragraph_motions() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"a\nb\n\nc\nd\n\n\ne\n");
        let next = buffer.find_next_paragraph(loc(0, 0));
        assert_eq!((next.line_idx, next.grapheme_idx), (2, 0));
        let next = buffer.find_next_paragraph(loc(2, 0));
        assert_eq!((next.line_idx, next.grapheme_idx), (5, 0));
        let next = buffer.find_next_paragraph(loc(6, 0));
        assert_eq!((next.line_idx, next.grapheme_idx), (7, 1));
        let prev = buffer.find_prev_paragraph(loc(4, 1));
        assert_eq!((prev.line_idx, prev.grapheme_idx), (2, 0));
        let prev = buffer.find_prev_paragraph(loc(3, 0));
        assert_eq!((prev.line_idx, prev.grapheme_idx), (2, 0));
        let prev = buffer.find_prev_paragraph(loc(1, 0));
        assert_eq!((prev.line_idx, prev.grapheme_idx), (0, 0));
    }

    #[test]
    fn punctuation_splits_words_but_not_bigwords() {
        let dir = TempDir::new();
        let (buffer, _) = open(&dir, "f.txt", b"foo.bar(x)\n");
        assert_eq!(buffer.find_next_word_start(loc(0, 0)).grapheme_idx, 3);
        assert_eq!(buffer.find_word_end(loc(0, 0)).grapheme_idx, 2);
        assert_eq!(buffer.find_next_bigword_start(loc(0, 0)).grapheme_idx, 10);
    }

    #[test]
    fn indent_style_detection() {
        let dir = TempDir::new();
        assert_eq!(open(&dir, "a.txt", b"\ta\n\tb\n c\n").0.indent_style(), "Tabs");
        assert_eq!(open(&dir, "b.txt", b"    a\n    b\n").0.indent_style(), "Spaces: 4");
    }
}