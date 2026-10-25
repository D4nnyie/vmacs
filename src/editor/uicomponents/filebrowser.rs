#![allow(clippy::arithmetic_side_effects, clippy::integer_division)]

use std::{
    env,
    fs::{self, File},
    io::{Error, Read},
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use super::super::theme as t;

use crate::prelude::*;

use super::super::{AnnotatedString, Annotation, StatusSpan, Terminal};
use super::{highlight_lines, UIComponent};

const BAR_BG: Color = t::SURFACE; const SELECTED_BG: Color = t::SURFACE_ACTIVE; const TEXT: Color = t::TEXT;
const DIM: Color = t::TEXT_DIM; const BLUE: Color = t::BLUE; const YELLOW: Color = t::AMBER; const SALMON: Color = t::RED;
const BADGE_FG: Color = t::ON_ACCENT; const BADGE_BG: Color = t::BADGE_NEUTRAL; const FILTER_BG: Color = t::SURFACE_FILTER;

const PREVIEW_MIN_WIDTH: usize = 60;
const PREVIEW_MAX_BYTES: u64 = 32 * 1024;
const PREVIEW_MAX_LINES: usize = 200;

struct Preview {
    path: PathBuf,
    title: String,
    lines: Vec<String>,
    notes: Vec<Vec<Annotation>>,
    note: Option<String>,
}

fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        if c == '\t' {
            out.push_str("    ");
        } else if c.is_control() {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn preview_line(line: &str, notes: &[Annotation], max: usize) -> (AnnotatedString, usize, bool) {
    let mut out = AnnotatedString::from(line);
    for note in notes {
        out.add_annotation(
            note.annotation_type,
            note.start.min(line.len()),
            note.end.min(line.len()),
        );
    }
    if line.width() <= max {
        return (out, line.width(), false);
    }
    let (mut used, mut cut) = (0, line.len());
    for (idx, ch) in line.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            cut = idx;
            break;
        }
        used += w;
    }
    out.truncate_right_from(cut);
    (out, used, true)
}

fn build_preview(path: &Path, show_hidden: bool) -> Preview {
    let title = path
        .file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut preview = Preview {
        path: path.to_path_buf(),
        title,
        lines: Vec::new(),
        notes: Vec::new(),
        note: None,
    };
    let Ok(meta) = fs::metadata(path) else {
        preview.note = Some("(cannot read)".to_string());
        return preview;
    };
    if meta.is_dir() {
        match fs::read_dir(path) {
            Ok(read) => {
                let mut items: Vec<(String, bool)> = read
                    .filter_map(Result::ok)
                    .filter_map(|item| {
                        let name = item.file_name().to_string_lossy().into_owned();
                        if !show_hidden && name.starts_with('.') {
                            return None;
                        }
                        let is_dir = fs::metadata(item.path()).is_ok_and(|m| m.is_dir());
                        Some((name, is_dir))
                    })
                    .collect();
                items.sort_by(|a, b| {
                    b.1.cmp(&a.1)
                        .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                });
                let extra = items.len().saturating_sub(PREVIEW_MAX_LINES);
                preview.lines = items
                    .into_iter()
                    .take(PREVIEW_MAX_LINES)
                    .map(|(name, is_dir)| {
                        clean_line(&if is_dir { format!("{name}/") } else { name })
                    })
                    .collect();
                if preview.lines.is_empty() {
                    preview.note = Some("(empty folder)".to_string());
                } else if extra > 0 {
                    preview.note = Some(format!("… and {extra} more"));
                }
            }
            Err(err) => preview.note = Some(format!("(cannot read folder: {err})")),
        }
        return preview;
    }
    if !meta.is_file() {
        preview.note = Some("(not a regular file)".to_string());
        return preview;
    }
    let size = meta.len();
    if size == 0 {
        preview.note = Some("(empty file)".to_string());
        return preview;
    }
    let mut bytes = Vec::new();
    let read = File::open(path).and_then(|file| file.take(PREVIEW_MAX_BYTES).read_to_end(&mut bytes));
    if let Err(err) = read {
        preview.note = Some(format!("(cannot read: {err})"));
        return preview;
    }
    let truncated = size > PREVIEW_MAX_BYTES;
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) if !bytes.contains(&0) => text.to_string(),
        // A cut-off multi-byte character at the end of a truncated read is fine.
        Err(err) if truncated && err.error_len().is_none() && !bytes.contains(&0) => {
            String::from_utf8_lossy(&bytes[..err.valid_up_to()]).into_owned()
        }
        _ => {
            preview.note = Some(format!("(binary file, {size} bytes)"));
            return preview;
        }
    };
    preview.lines = text.lines().take(PREVIEW_MAX_LINES).map(clean_line).collect();
    preview.notes = highlight_lines(&path.to_string_lossy(), &preview.lines);
    if truncated {
        preview.note = Some(format!("… ({size} bytes, showing the start)"));
    }
    preview
}

pub enum BrowserAction {
    None,
    Close,
    Open(PathBuf),
}

#[derive(Clone)]
struct Entry {
    name: String,
    is_dir: bool,
}

#[derive(Default)]
pub struct FileBrowser {
    dir: PathBuf,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    selected: usize,
    scroll: usize,
    show_hidden: bool,
    filter: String,
    filtering: bool,
    error: Option<String>,
    size: Size,
    needs_redraw: bool,
    preview: Option<Preview>,
    compact: bool,
    boxed: bool,
    outer: Size,
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" || input.starts_with("~/") || input.starts_with("~\\") {
        if let Some(home) = home_dir() {
            let rest = input.trim_start_matches('~').trim_start_matches(['/', '\\']);
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}

fn truncate(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn truncate_left(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for ch in text.chars().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        tail.push(ch);
        used += w;
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

impl FileBrowser {
    pub fn set_compact(&mut self, on: bool) {
        if self.compact != on {
            self.compact = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_boxed(&mut self, on: bool) {
        if self.boxed != on { self.boxed = on; self.set_size(self.outer); self.set_needs_redraw(true); }
    }

    fn frame(&self) -> Option<(usize, usize, usize, usize)> {
        let o = self.outer;
        if !self.boxed || o.width < 44 || o.height < 12 { return None; }
        let w = o.width.saturating_sub(4).min(120);
        let h = o.height.saturating_sub(2).min(38);
        Some((o.width.saturating_sub(w) / 2, o.height.saturating_sub(h) / 2, w, h))
    }

    pub fn open(&mut self, start: &Path) {
        let dir = if start.is_dir() {
            start.to_path_buf()
        } else {
            start.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        self.dir = fs::canonicalize(&dir)
            .or_else(|_| env::current_dir())
            .unwrap_or_default();
        self.filter.clear();
        self.filtering = false;
        let select = if start.is_dir() {
            None
        } else {
            start.file_name().map(|name| name.to_string_lossy().into_owned())
        };
        self.load(select.as_deref());
        self.set_needs_redraw(true);
    }

    pub fn scroll(&mut self, delta: isize) {
        self.move_selection(delta);
        self.set_needs_redraw(true);
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> BrowserAction {
        self.set_needs_redraw(true);
        if modifiers == KeyModifiers::CONTROL {
            match code {
                KeyCode::Char('n') => self.move_selection(1),
                KeyCode::Char('p') => self.move_selection(-1),
                KeyCode::Char('h') => self.backspace(),
                _ => {}
            }
            return BrowserAction::None;
        }
        if !matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
            return BrowserAction::None;
        }
        match code {
            KeyCode::Esc => return self.escape(),
            KeyCode::Enter | KeyCode::Right => return self.activate(),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(self.page_step()),
            KeyCode::PageUp => self.move_selection(-self.page_step()),
            KeyCode::Home => self.select_index(0),
            KeyCode::End => self.select_index(usize::MAX),
            KeyCode::Left => self.go_parent(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Char(c) if self.filtering => {
                self.filter.push(c);
                self.apply_filter();
            }
            KeyCode::Char(c) => return self.handle_char(c),
            _ => {}
        }
        BrowserAction::None
    }

    fn handle_char(&mut self, c: char) -> BrowserAction {
        match c {
            'q' => return BrowserAction::Close,
            'j' => self.move_selection(1),
            'k' => self.move_selection(-1),
            'g' => self.select_index(0),
            'G' => self.select_index(usize::MAX),
            'l' => return self.activate(),
            'h' => self.go_parent(),
            '/' => self.filtering = true,
            '.' => self.toggle_hidden(),
            '~' => self.go_home(),
            _ => {}
        }
        BrowserAction::None
    }

    fn escape(&mut self) -> BrowserAction {
        if self.filtering {
            self.filtering = false;
            self.filter.clear();
            self.apply_filter();
            BrowserAction::None
        } else {
            BrowserAction::Close
        }
    }

    fn current(&self) -> Option<&Entry> {
        self.filtered
            .get(self.selected)
            .and_then(|&index| self.entries.get(index))
    }

    fn activate(&mut self) -> BrowserAction {
        let Some(entry) = self.current().cloned() else {
            return BrowserAction::None;
        };
        if entry.name == ".." {
            self.go_parent();
            return BrowserAction::None;
        }
        let path = self.dir.join(&entry.name);
        if entry.is_dir {
            self.dir = path;
            self.filter.clear();
            self.filtering = false;
            self.load(None);
            BrowserAction::None
        } else {
            BrowserAction::Open(path)
        }
    }

    fn go_parent(&mut self) {
        let Some(parent) = self.dir.parent().map(Path::to_path_buf) else {
            return;
        };
        let came_from = self
            .dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        self.dir = parent;
        self.filter.clear();
        self.filtering = false;
        self.load(came_from.as_deref());
    }

    fn go_home(&mut self) {
        if let Some(home) = home_dir().filter(|home| home.is_dir()) {
            self.dir = home;
            self.filter.clear();
            self.filtering = false;
            self.load(None);
        }
    }

    fn toggle_hidden(&mut self) {
        let keep = self.current().map(|entry| entry.name.clone());
        self.show_hidden = !self.show_hidden;
        self.load(keep.as_deref());
    }

    fn load(&mut self, select: Option<&str>) {
        self.error = None;
        self.preview = None;
        self.entries.clear();
        if self.dir.parent().is_some() {
            self.entries.push(Entry { name: "..".to_string(), is_dir: true });
        }
        let show_hidden = self.show_hidden;
        match fs::read_dir(&self.dir) {
            Ok(read) => {
                let mut found: Vec<Entry> = read
                    .filter_map(Result::ok)
                    .filter_map(|item| {
                        let name = item.file_name().to_string_lossy().into_owned();
                        if !show_hidden && name.starts_with('.') {
                            return None;
                        }
                        let is_dir = fs::metadata(item.path()).is_ok_and(|meta| meta.is_dir());
                        Some(Entry { name, is_dir })
                    })
                    .collect();
                found.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                self.entries.extend(found);
            }
            Err(err) => self.error = Some(format!("Cannot read directory: {err}")),
        }
        self.apply_filter();
        if let Some(name) = select {
            if let Some(pos) = self.filtered.iter().position(|&index| {
                self.entries.get(index).is_some_and(|entry| entry.name == name)
            }) {
                self.selected = pos;
            }
        }
        self.ensure_visible();
    }

    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                needle.is_empty()
                    || (entry.name != ".." && entry.name.to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll = 0;
    }

    fn list_rows(&self) -> usize {
        self.size.height.saturating_sub(3)
    }

    fn page_step(&self) -> isize {
        isize::try_from(self.list_rows().saturating_sub(1).max(1)).unwrap_or(1)
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered.len();
        if len == 0 {
            return;
        }
        self.selected = if delta < 0 {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected.saturating_add(delta.unsigned_abs()).min(len - 1)
        };
        self.ensure_visible();
    }

    fn select_index(&mut self, index: usize) {
        let len = self.filtered.len();
        if len == 0 {
            return;
        }
        self.selected = index.min(len - 1);
        self.ensure_visible();
    }

    fn ensure_visible(&mut self) {
        let rows = self.list_rows().max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }
    }

    fn backspace(&mut self) {
        if self.filtering {
            if self.filter.pop().is_some() {
                self.apply_filter();
            }
        } else {
            self.go_parent();
        }
    }

    fn refresh_preview(&mut self) {
        let Some(entry) = self.current().cloned() else {
            self.preview = None;
            return;
        };
        let path = self.dir.join(&entry.name);
        if self.preview.as_ref().is_some_and(|p| p.path == path) {
            return;
        }
        self.preview = Some(if entry.name == ".." {
            Preview {
                path,
                title: "..".to_string(),
                lines: Vec::new(),
                notes: Vec::new(),
                note: Some("(parent folder)".to_string()),
            }
        } else {
            build_preview(&path, self.show_hidden)
        });
    }

    fn draw_filter_bar(&self, row: RowIdx, width: usize) -> Result<(), Error> {
        let total = self.entries.iter().filter(|e| e.name != "..").count();
        let (bar_bg, badge_bg) = if self.filtering {
            (FILTER_BG, YELLOW)
        } else {
            (BAR_BG, BADGE_BG)
        };
        let badge = if self.compact { " / " } else { " FILTER " };
        let badge_w = badge.width();
        let count = if self.filtering {
            format!("{}/{} ", self.filtered.len(), total)
        } else {
            String::new()
        };
        let count_w = count.width();
        let room = width.saturating_sub(badge_w + count_w + 3);
        let mut spans = vec![StatusSpan {
            col: 0,
            text: badge.to_string(),
            fg: BADGE_FG,
            bg: badge_bg,
            bold: true,
        }];
        if self.filtering {
            spans.push(StatusSpan {
                col: badge_w + 1,
                text: format!("{}▏", truncate_left(&self.filter, room)),
                fg: TEXT,
                bg: bar_bg,
                bold: true,
            });
            if count_w > 0 && width > badge_w + count_w + 2 {
                spans.push(StatusSpan {
                    col: width - count_w,
                    text: count,
                    fg: YELLOW,
                    bg: bar_bg,
                    bold: true,
                });
            }
        } else {
            spans.push(StatusSpan {
                col: badge_w + 1,
                text: truncate(
                    if self.compact { "press /" } else { "press / to filter this folder" },
                    room,
                ),
                fg: DIM,
                bg: bar_bg,
                bold: false,
            });
        }
        Terminal::print_status_row(row, width, bar_bg, TEXT, &spans)
    }

    fn draw_preview_row(&self, row: RowIdx, i: usize, list_w: usize, width: usize) -> Result<(), Error> {
        let sep = if self.compact { "│" } else { "│ " };
        let col = list_w + sep.width();
        let pw = width.saturating_sub(col);
        Terminal::print_styled_at(Position { row, col: list_w }, sep, DIM, Color::Reset, false)?;
        let (text, fg, bold) = match self.preview.as_ref() {
            Some(p) if i == 0 => (p.title.clone(), BLUE, true),
            Some(p) => match p.lines.get(i - 1) {
                Some(line) => {
                    let notes = p.notes.get(i - 1).map(Vec::as_slice).unwrap_or(&[]);
                    let (styled, shown, cut) = preview_line(line, notes, pw);
                    Terminal::print_annotated_row(row, col, &styled, None, false)?;
                    if cut {
                        Terminal::print_styled_at(
                            Position { row, col: col + shown },
                            "…",
                            DIM,
                            Color::Reset,
                            false,
                        )?;
                    }
                    return Ok(());
                }
                None if i - 1 == p.lines.len() => (p.note.clone().unwrap_or_default(), DIM, false),
                None => (String::new(), TEXT, false),
            },
            None => (String::new(), TEXT, false),
        };
        let shown = truncate(&text, pw);
        let pad = pw.saturating_sub(shown.width());
        Terminal::print_styled_at(
            Position { row, col },
            &format!("{shown}{}", " ".repeat(pad)),
            fg,
            Color::Reset,
            bold,
        )
    }

    fn draw_inner(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let Size { height, width } = self.size;
        if height < 4 || width < 12 {
            return Ok(());
        }
        self.ensure_visible();

        let show_preview = width >= if self.compact { 50 } else { PREVIEW_MIN_WIDTH };
        let list_w = if !show_preview {
            width
        } else if self.compact {
            (width / 3).max(20)
        } else {
            (width * 2 / 5).max(24)
        };
        if show_preview {
            self.refresh_preview();
        }

        let dir_text = truncate_left(&self.dir.to_string_lossy(), width.saturating_sub(4));
        let header = [StatusSpan { col: 1, text: dir_text, fg: BLUE, bg: BAR_BG, bold: true }];
        Terminal::print_status_row(origin_row, width, BAR_BG, TEXT, &header)?;
        self.draw_filter_bar(origin_row + 1, width)?;

        let rows = height - 3;
        for i in 0..rows {
            let row = origin_row + 2 + i;
            let index = self.scroll + i;
            match self.filtered.get(index).and_then(|&e| self.entries.get(e)) {
                Some(entry) => {
                    let selected = index == self.selected;
                    let bg = if selected { SELECTED_BG } else { Color::Reset };
                    let (text, fg) = if entry.is_dir {
                        (format!("{}/", entry.name), BLUE)
                    } else {
                        (entry.name.clone(), TEXT)
                    };
                    let spans = [
                        StatusSpan {
                            col: 0,
                            text: if selected { "▌".to_string() } else { " ".to_string() },
                            fg: BLUE,
                            bg,
                            bold: false,
                        },
                        StatusSpan {
                            col: 2,
                            text: truncate(&text, list_w.saturating_sub(3)),
                            fg,
                            bg,
                            bold: selected,
                        },
                    ];
                    Terminal::print_status_row(row, list_w, bg, TEXT, &spans)?;
                }
                None => {
                    let mut spans = Vec::new();
                    if i == 0 && self.filtered.is_empty() {
                        let text = if self.filter.is_empty() { "(empty)" } else { "No matches" };
                        spans.push(StatusSpan {
                            col: 2,
                            text: text.to_string(),
                            fg: DIM,
                            bg: Color::Reset,
                            bold: false,
                        });
                    }
                    Terminal::print_status_row(row, list_w, Color::Reset, TEXT, &spans)?;
                }
            }
            if show_preview {
                self.draw_preview_row(row, i, list_w, width)?;
            }
        }

        let (hint, hint_fg) = if let Some(error) = &self.error {
            (format!(" {error}"), SALMON)
        } else if self.filtering {
            (" type to filter · Enter open · Esc clear".to_string(), DIM)
        } else if self.compact {
            (" j/k Enter h / . ~ q".to_string(), DIM)
        } else {
            (
                " j/k move · Enter open · h back · / filter · . hidden · ~ home · q close".to_string(),
                DIM,
            )
        };
        let count = format!(
            "{}/{}",
            if self.filtered.is_empty() { 0 } else { self.selected + 1 },
            self.filtered.len()
        );
        let count_w = count.width();
        let footer = [
            StatusSpan {
                col: 0,
                text: truncate(&hint, width.saturating_sub(count_w + 3)),
                fg: hint_fg,
                bg: BAR_BG,
                bold: false,
            },
            StatusSpan {
                col: width.saturating_sub(count_w + 1),
                text: count,
                fg: TEXT,
                bg: BAR_BG,
                bold: false,
            },
        ];
        Terminal::print_status_row(origin_row + height - 1, width, BAR_BG, TEXT, &footer)
    }

}

impl UIComponent for FileBrowser {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.outer = size;
        self.size = match self.frame() {
            Some((_, _, w, h)) => Size { width: w - 2, height: h - 2 },
            None => size,
        };
        self.ensure_visible();
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let Some((c, r, w, h)) = self.frame() else {
            return self.draw_inner(origin_row);
        };
        for i in 0..self.outer.height {
            Terminal::print_row(origin_row + i, "")?;
        }
        Terminal::set_origin(c + 1, origin_row + r + 1);
        let result = self.draw_inner(0);
        Terminal::set_origin(0, 0);
        result?;
        Terminal::draw_box(origin_row + r, c, w, h, "Open file", BLUE)
    }
}