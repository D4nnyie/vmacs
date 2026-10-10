#![allow(clippy::arithmetic_side_effects, clippy::integer_division)]

use std::{
    env, fs,
    io::Error,
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::prelude::*;

use super::super::{StatusSpan, Terminal};
use super::UIComponent;

const BAR_BG: Color = Color::Rgb { r: 0x1a, g: 0x1e, b: 0x27 };
const SELECTED_BG: Color = Color::Rgb { r: 0x2a, g: 0x3b, b: 0x59 };
const TEXT: Color = Color::Rgb { r: 0xc5, g: 0xcf, b: 0xdb };
const DIM: Color = Color::Rgb { r: 0x63, g: 0x75, b: 0x8a };
const BLUE: Color = Color::Rgb { r: 0x82, g: 0xaa, b: 0xff };
const YELLOW: Color = Color::Rgb { r: 0xf6, g: 0xc8, b: 0x5f };
const SALMON: Color = Color::Rgb { r: 0xf2, g: 0x8f, b: 0x79 };

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
        self.size.height.saturating_sub(2)
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
}

impl UIComponent for FileBrowser {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
        self.ensure_visible();
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let Size { height, width } = self.size;
        if height < 3 || width < 12 {
            return Ok(());
        }
        self.ensure_visible();

        let filter_text = if self.filtering {
            format!("{}▏", truncate_left(&format!("/{}", self.filter), width / 2))
        } else {
            String::new()
        };
        let filter_w = filter_text.width();
        let dir_text = truncate_left(
            &self.dir.to_string_lossy(),
            width.saturating_sub(filter_w + 4),
        );
        let mut header = vec![StatusSpan { col: 1, text: dir_text, fg: BLUE, bg: BAR_BG, bold: true }];
        if filter_w > 0 {
            header.push(StatusSpan {
                col: width.saturating_sub(filter_w + 1),
                text: filter_text,
                fg: YELLOW,
                bg: BAR_BG,
                bold: true,
            });
        }
        Terminal::print_status_row(origin_row, width, BAR_BG, TEXT, &header)?;

        let rows = height - 2;
        for i in 0..rows {
            let row = origin_row + 1 + i;
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
                            text: truncate(&text, width.saturating_sub(3)),
                            fg,
                            bg,
                            bold: selected,
                        },
                    ];
                    Terminal::print_status_row(row, width, bg, TEXT, &spans)?;
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
                    Terminal::print_status_row(row, width, Color::Reset, TEXT, &spans)?;
                }
            }
        }

        let (hint, hint_fg) = if let Some(error) = &self.error {
            (format!(" {error}"), SALMON)
        } else if self.filtering {
            (" type to filter · Enter open · Esc clear".to_string(), DIM)
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