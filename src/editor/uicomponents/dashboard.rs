#![allow(clippy::integer_division, clippy::arithmetic_side_effects)]

use std::{env, io::Error};

use crossterm::event::{KeyCode, KeyModifiers};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::prelude::*;

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use super::super::config::{DashLayout, IconMode};
mod layouts;
use super::super::{AnnotatedString, AnnotationType, Terminal};
use super::UIComponent;

const LOGO: &[&str] = &[
    "██╗   ██╗███╗   ███╗ █████╗  ██████╗███████╗",
    "██║   ██║████╗ ████║██╔══██╗██╔════╝██╔════╝",
    "██║   ██║██╔████╔██║███████║██║     ███████╗",
    "╚██╗ ██╔╝██║╚██╔╝██║██╔══██║██║     ╚════██║",
    " ╚████╔╝ ██║ ╚═╝ ██║██║  ██║╚██████╗███████║",
    "  ╚═══╝  ╚═╝     ╚═╝╚═╝  ╚═╝ ╚═════╝╚══════╝",
];
const LOGO_WIDTH: usize = 44;

const MENU_WIDTH: usize = 38;
const MAX_RECENT: usize = 10;
const MAX_VISIBLE: usize = 20;
const FILTER_SHOWN: usize = 24;

const COL_W: usize = 36;
const MIN_COL: usize = 22;
const COL_GAP: usize = 4;

const KEY_W: usize = 3;

const RECENT_ICON: (&str, &str) = ("▪ ", "\u{f0f6} ");
const RECENT_HEADING_ICON: (&str, &str) = ("↺ ", "\u{f1da} ");
const PIN_ICON: (&str, &str) = ("★ ", "\u{f08d} ");

const OPEN_MS: f32 = 180.0;
static SEL_STAGE: AtomicU8 = AtomicU8::new(0);

fn selection_kind() -> AnnotationType {
    match SEL_STAGE.load(Ordering::Relaxed) {
        1 => AnnotationType::Flash,
        2 => AnnotationType::FlashSoft,
        _ => AnnotationType::Selection,
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum IconSet {
    #[default]
    None,
    Plain,
    Nerd,
}

/// Best-effort guess; a terminal cannot report which glyphs its font has.
pub(super) fn terminal_supports_icons() -> bool {
    let term = env::var("TERM").unwrap_or_default();
    if term == "linux" || term == "dumb" {
        return false;
    }
    if cfg!(windows) {
        return env::var_os("WT_SESSION").is_some() || env::var_os("TERM_PROGRAM").is_some();
    }
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|key| env::var(key).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}

/// Shift+digit on a US layout arrives as a symbol.
const fn shifted_digit(c: char) -> Option<char> {
    match c {
        '!' => Some('1'),
        '@' => Some('2'),
        '#' => Some('3'),
        '$' => Some('4'),
        '%' => Some('5'),
        '^' => Some('6'),
        '&' => Some('7'),
        '*' => Some('8'),
        '(' => Some('9'),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecentKey {
    Ignored,
    Handled,
    Open(String),
    TogglePin(String),
    /// Enter on a menu row: the key that row stands for.
    Menu(char),
}

struct MenuItem {
    icon: (&'static str, &'static str),
    label: String,
    key: &'static str,
    action: char,
}

#[derive(Clone)]
struct Cell {
    text: String,
    notes: Vec<(AnnotationType, usize, usize)>,
}

impl Cell {
    fn blank(width: usize) -> Self {
        Self { text: " ".repeat(width), notes: Vec::new() }
    }

    fn text(text: &str, kind: AnnotationType, width: usize) -> Self {
        let pad = width.saturating_sub(text.width());
        Self {
            text: format!("{text}{}", " ".repeat(pad)),
            notes: vec![(kind, 0, text.len())],
        }
    }
}

#[derive(Default)]
pub struct Dashboard {
    pinned: Vec<String>,
    recent: Vec<String>,
    pinned_f: Vec<usize>,
    recent_f: Vec<usize>,
    active_pinned: bool,
    selected: Option<usize>,
    filter: String,
    filtering: bool,
    pending_recent: bool,
    size: Size,
    needs_redraw: bool,
    icons: IconSet,
    menu_sel: Option<usize>,
    session_files: usize,
    compact: bool,
    workstation: bool,
    boxed: bool,
    layout: DashLayout,
    animate: bool,
    open_anim: Option<Instant>,
    sel_anim: Option<Instant>,
}

fn fit(text: &str, max: usize) -> String {
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

impl Dashboard {


    pub fn set_session(&mut self, files: usize) {
        self.session_files = files;
        self.menu_sel = self.menu_sel.map(|i| i.min(self.menu_len() - 1));
        self.set_needs_redraw(true);
    }

    pub fn set_compact(&mut self, on: bool) {
        if self.compact != on {
            self.compact = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_workstation(&mut self, on: bool) {
        if self.workstation != on { self.workstation = on; self.set_needs_redraw(true); }
    }

    fn column_for(&self, pinned: bool, w: usize, budget: usize) -> Vec<Cell> {
        if self.workstation { self.column_ws(pinned, w, budget) } else { self.column(pinned, w, budget) }
    }

    fn menu_len(&self) -> usize {
        8 + usize::from(self.session_files > 0)
    }

    fn menu(&self) -> Vec<MenuItem> {
        let item = |icon, label: &str, key, action| MenuItem {
            icon,
            label: label.to_string(),
            key,
            action,
        };
        let mut items = vec![
            item(("+ ", "\u{f067} "), "New File", "n/i", 'n'),
            item(("▤ ", "\u{f07c} "), "Open File", "e", 'e'),
        ];
        if self.session_files > 0 {
            items.push(item(
                ("↻ ", "\u{f021} "),
                &format!("Last Session ({})", self.session_files),
                "s",
                's',
            ));
        }
        items.push(item(("◆ ", "\u{f013} "), "Edit config.toml", "c", 'c'));
        items.push(item(("▣ ", "\u{f07b} "), "Config Folder", "d", 'd'));
        items.push(item(("λ ", "\u{f121} "), "New Language", "g", 'g'));
        items.push(item(("# ", "\u{f11c} "), "Keys & Actions", "m", 'm'));
        items.push(item(("? ", "\u{f059} "), "Help", "h", 'h'));
        items.push(item(("× ", "\u{f011} "), "Quit", "q", 'q'));
        items
    }

    pub fn set_boxed(&mut self, on: bool) {
        if self.boxed != on { self.boxed = on; self.set_needs_redraw(true); }
    }

    pub fn set_layout(&mut self, layout: DashLayout) {
        if self.layout != layout { self.layout = layout; self.set_needs_redraw(true); }
    }

    pub fn set_icons(&mut self, mode: IconMode) {
        self.icons = match mode {
            IconMode::Off => IconSet::None,
            IconMode::Nerd => IconSet::Nerd,
            IconMode::On => IconSet::Plain,
            IconMode::Auto if terminal_supports_icons() => IconSet::Plain,
            IconMode::Auto => IconSet::None,
        };
        self.set_needs_redraw(true);
    }

    fn icon(&self, (plain, nerd): (&'static str, &'static str)) -> &'static str {
        match self.icons {
            IconSet::None => "",
            IconSet::Plain => plain,
            IconSet::Nerd => nerd,
        }
    }

    fn has_entries(&self) -> bool {
        !self.pinned.is_empty() || !self.recent.is_empty()
    }

    pub fn set_entries(&mut self, pinned: Vec<String>, recent: Vec<String>) {
        let keep = self.selected_path();
        self.recent = recent
            .into_iter()
            .filter(|path| !pinned.contains(path))
            .take(MAX_RECENT)
            .collect();
        self.pinned = pinned;
        self.refilter();
        self.active_pinned = !self.pinned_f.is_empty();
        self.selected = None;
        if let Some(path) = keep {
            if let Some(pos) = self.find(true, &path) {
                self.active_pinned = true;
                self.selected = Some(pos);
            } else if let Some(pos) = self.find(false, &path) {
                self.active_pinned = false;
                self.selected = Some(pos);
            }
        }
        if !self.has_entries() {
            self.filtering = false;
            self.filter.clear();
            self.refilter();
        } else if self.filtering && self.selected.is_none() && self.any_filtered() {
            self.selected = Some(0);
        }
        self.set_needs_redraw(true);
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.filtering = false;
        self.pending_recent = false;
        self.apply_filter();
        self.set_needs_redraw(true);
    }

    pub fn remove_entry(&mut self, path: &str) {
        self.pinned.retain(|p| p != path);
        self.recent.retain(|p| p != path);
        self.refilter();
        if self.pinned_f.is_empty() {
            self.active_pinned = false;
        }
        let len = self.active_len();
        self.selected = match self.selected {
            Some(_) if len == 0 => None,
            Some(i) => Some(i.min(len - 1)),
            None => None,
        };
        if !self.has_entries() {
            self.filtering = false;
            self.filter.clear();
            self.refilter();
        }
        self.set_needs_redraw(true);
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> RecentKey {
        let before = (self.menu_sel, self.selected, self.active_pinned);
        let out = self.handle_key_inner(code, modifiers);
        if self.animate && before != (self.menu_sel, self.selected, self.active_pinned) {
            self.sel_anim = Some(Instant::now());
        }
        out
    }

    fn handle_key_inner(&mut self, code: KeyCode, modifiers: KeyModifiers) -> RecentKey {
        self.set_needs_redraw(true);
        let plain = matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT);
        let ctrl = modifiers == KeyModifiers::CONTROL;

        if self.filtering {
            if matches!(modifiers, KeyModifiers::ALT | KeyModifiers::META)
                && code == KeyCode::Char('p')
            {
                return self.toggle_selected();
            }
            if ctrl {
                match code {
                    KeyCode::Char('n') => self.move_in_list(true),
                    KeyCode::Char('p') => self.move_in_list(false),
                    KeyCode::Char('h') => self.filter_backspace(),
                    _ => {}
                }
                return RecentKey::Handled;
            }
            if !plain {
                return RecentKey::Handled;
            }
            match code {
                KeyCode::Esc => self.stop_filter(),
                KeyCode::Enter => return self.open_selected(),
                KeyCode::Down => self.move_in_list(true),
                KeyCode::Up => self.move_in_list(false),
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    self.switch_column();
                }
                KeyCode::Backspace => self.filter_backspace(),
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.apply_filter();
                }
                _ => {}
            }
            return RecentKey::Handled;
        }

        let awaiting = std::mem::take(&mut self.pending_recent);
        if awaiting && plain {
            if let KeyCode::Char(digit @ '0'..='9') = code {
                return self.open_digit(false, digit);
            }
        }

        if ctrl {
            return match code {
                KeyCode::Char('n') => {
                    self.move_selection(true);
                    RecentKey::Handled
                }
                KeyCode::Char('p') => {
                    self.move_selection(false);
                    RecentKey::Handled
                }
                _ => RecentKey::Ignored,
            };
        }
        if !plain {
            return RecentKey::Ignored;
        }
        match code {
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(true),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(false),
            KeyCode::Enter | KeyCode::Char('l') => return self.open_selected(),
            KeyCode::Char('p') => return self.toggle_selected(),
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                if self.menu_sel.is_none() {
                    self.switch_column();
                }
            }
            KeyCode::Esc if self.menu_sel.is_some() || self.selected.is_some() => {
                self.menu_sel = None;
                self.selected = None;
            }
            KeyCode::Char('/') if self.has_entries() => {
                self.filtering = true;
                self.apply_filter();
            }
            KeyCode::Char('r' | 'R') if self.has_entries() => self.pending_recent = true,
            KeyCode::Char(digit @ '1'..='9') => {
                let recent = modifiers.contains(KeyModifiers::SHIFT);
                return self.open_digit(!recent, digit);
            }
            KeyCode::Char(letter @ 'A'..='K') => {
                let pos = usize::from(u8::try_from(letter).unwrap_or(b'A').saturating_sub(b'A'))
                    .saturating_add(9);
                return self.open_pos(true, pos);
            }
            KeyCode::Char(symbol) => match shifted_digit(symbol) {
                Some(digit) => return self.open_digit(false, digit),
                None => return RecentKey::Ignored,
            },
            _ => return RecentKey::Ignored,
        }
        RecentKey::Handled
    }

    fn open_pos(&self, pinned: bool, pos: usize) -> RecentKey {
        self.path_in(pinned, pos)
            .map_or(RecentKey::Handled, RecentKey::Open)
    }

    fn open_digit(&self, pinned: bool, digit: char) -> RecentKey {
        let pos = match digit {
            '0' => 9,
            d => d
                .to_digit(10)
                .and_then(|n| usize::try_from(n).ok())
                .map_or(0, |n| n.saturating_sub(1)),
        };
        self.open_pos(pinned, pos)
    }

    fn key_label(pinned: bool, pos: usize) -> String {
        match (pinned, pos) {
            (true, 0..=8) => pos.saturating_add(1).to_string(),
            (true, 9..=19) => {
                let letter = u8::try_from(pos.saturating_sub(9)).unwrap_or(0);
                char::from(b'A'.saturating_add(letter)).to_string()
            }
            (false, 0..=8) => format!("R{}", pos.saturating_add(1)),
            (false, 9) => "R0".to_string(),
            _ => String::new(),
        }
    }

    fn path_in(&self, pinned: bool, pos: usize) -> Option<String> {
        let (list, filtered) = if pinned {
            (&self.pinned, &self.pinned_f)
        } else {
            (&self.recent, &self.recent_f)
        };
        filtered.get(pos).and_then(|&index| list.get(index)).cloned()
    }

    fn find(&self, pinned: bool, path: &str) -> Option<usize> {
        let (list, filtered) = if pinned {
            (&self.pinned, &self.pinned_f)
        } else {
            (&self.recent, &self.recent_f)
        };
        filtered
            .iter()
            .position(|&index| list.get(index).is_some_and(|p| p == path))
    }

    fn active_len(&self) -> usize {
        if self.active_pinned { self.pinned_f.len() } else { self.recent_f.len() }
    }

    fn any_filtered(&self) -> bool {
        !self.pinned_f.is_empty() || !self.recent_f.is_empty()
    }

    fn selected_path(&self) -> Option<String> {
        self.selected.and_then(|pos| self.path_in(self.active_pinned, pos))
    }

    fn open_selected(&self) -> RecentKey {
        if let Some(i) = self.menu_sel {
            return self
                .menu()
                .get(i)
                .map_or(RecentKey::Handled, |item| RecentKey::Menu(item.action));
        }
        self.selected_path().map_or(RecentKey::Handled, RecentKey::Open)
    }

    fn toggle_selected(&self) -> RecentKey {
        if self.menu_sel.is_some() {
            return RecentKey::Handled;
        }
        self.selected_path().map_or(RecentKey::Handled, RecentKey::TogglePin)
    }

    fn move_selection(&mut self, forward: bool) {
        if self.filtering {
            self.move_in_list(forward);
            return;
        }
        let menu = self.menu_len();
        let list = self.active_len();
        match (self.menu_sel, self.selected) {
            (Some(i), _) if forward => {
                if i + 1 < menu {
                    self.menu_sel = Some(i + 1);
                } else if list > 0 {
                    self.menu_sel = None;
                    self.selected = Some(0);
                }
            }
            (Some(i), _) => self.menu_sel = Some(i.saturating_sub(1)),
            (None, Some(i)) if forward => {
                self.selected = Some((i + 1).min(list.saturating_sub(1)));
            }
            (None, Some(0)) => {
                self.menu_sel = Some(menu - 1);
                self.selected = None;
            }
            (None, Some(i)) => self.selected = Some(i - 1),
            (None, None) if forward => self.menu_sel = Some(0),
            (None, None) => {
                if list > 0 {
                    self.selected = Some(list - 1);
                } else {
                    self.menu_sel = Some(menu - 1);
                }
            }
        }
    }

    fn move_in_list(&mut self, forward: bool) {
        let len = self.active_len();
        if len == 0 {
            return;
        }
        let last = len - 1;
        self.selected = Some(match (self.selected, forward) {
            (None, true) => 0,
            (None, false) => last,
            (Some(i), true) => (i + 1).min(last),
            (Some(i), false) => i.saturating_sub(1),
        });
    }

    fn switch_column(&mut self) {
        let other = !self.active_pinned;
        let other_len = if other { self.pinned_f.len() } else { self.recent_f.len() };
        if other_len == 0 {
            return;
        }
        let pos = self.selected.unwrap_or(0).min(other_len - 1);
        self.active_pinned = other;
        self.selected = Some(pos);
    }

    fn stop_filter(&mut self) {
        self.filtering = false;
        self.filter.clear();
        self.apply_filter();
    }

    fn filter_backspace(&mut self) {
        if self.filter.pop().is_some() {
            self.apply_filter();
        } else {
            self.stop_filter();
        }
    }

    fn refilter(&mut self) {
        let terms: Vec<String> = self
            .filter
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let matching = |list: &[String]| -> Vec<usize> {
            list.iter()
                .enumerate()
                .filter(|(_, path)| {
                    let path = path.to_lowercase();
                    terms.iter().all(|term| path.contains(term.as_str()))
                })
                .map(|(index, _)| index)
                .collect()
        };
        self.pinned_f = matching(&self.pinned);
        self.recent_f = matching(&self.recent);
    }

    fn apply_filter(&mut self) {
        self.menu_sel = None;
        self.refilter();
        self.active_pinned = !self.pinned_f.is_empty();
        self.selected = if self.filtering && self.any_filtered() {
            Some(0)
        } else {
            None
        };
    }

    fn label_for(path: &str, max: usize) -> String {
        let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok();
        let mut label = match &home {
            Some(home) if !home.is_empty()
                && path.starts_with(home.as_str())
                && path[home.len()..].starts_with(['/', '\\']) =>
            {
                format!("~{}", &path[home.len()..])
            }
            _ => path.to_string(),
        };
        if label.chars().count() > max {
            let tail: Vec<char> = label.chars().rev().take(max.saturating_sub(3)).collect();
            label = format!("...{}", tail.into_iter().rev().collect::<String>());
        }
        label
    }

    fn block_pad(&self) -> usize {
        self.size.width.saturating_sub(MENU_WIDTH) / 2
    }

    fn centered(&self, text: &str, kind: Option<AnnotationType>) -> AnnotatedString {
        let pad = self.size.width.saturating_sub(text.width()) / 2;
        let line = format!("{}{text}", " ".repeat(pad));
        let mut row = AnnotatedString::from(&line);
        if let Some(kind) = kind {
            row.add_annotation(kind, pad, line.len());
        }
        row
    }

    fn cell(&self, icon: &str, label: &str, key: &str, selected: bool, width: usize) -> Cell {
        let left = if icon.is_empty() {
            label.to_string()
        } else {
            format!("{icon} {label}")
        };
        let room = width.saturating_sub(KEY_W + 1);
        let left = fit(&left, room);
        let pad = room.saturating_sub(left.width());
        let text = format!("{left}{} {key:>KEY_W$}", " ".repeat(pad));
        let mut notes = Vec::new();
        if !key.is_empty() {
            notes.push((AnnotationType::Number, text.len().saturating_sub(key.len()), text.len()));
        }
        if selected {
            notes.push((selection_kind(), 0, text.len()));
        }
        Cell { text, notes }
    }

    fn compose(pad: usize, cells: Vec<Cell>) -> AnnotatedString {
        let mut line = " ".repeat(pad);
        let mut notes: Vec<(AnnotationType, usize, usize)> = Vec::new();
        for (n, cell) in cells.into_iter().enumerate() {
            if n > 0 {
                line.push_str(&" ".repeat(COL_GAP));
            }
            let base = line.len();
            for (kind, start, end) in cell.notes {
                notes.push((kind, base + start, base + end));
            }
            line.push_str(&cell.text);
        }
        let mut row = AnnotatedString::from(&line);
        for (kind, start, end) in notes {
            row.add_annotation(kind, start, end);
        }
        row
    }

    fn menu_row(&self, icon: &str, label: &str, key: &str, selected: bool) -> AnnotatedString {
        Self::compose(self.block_pad(), vec![self.cell(icon, label, key, selected, MENU_WIDTH)])
    }

    fn heading(&self, pinned: bool) -> String {
        let (icon, title) = if pinned {
            (self.icon(PIN_ICON), "Pinned")
        } else {
            (self.icon(RECENT_HEADING_ICON), "Recent Files")
        };
        if icon.is_empty() {
            title.to_string()
        } else {
            format!("{icon} {title}")
        }
    }

    fn filter_text(&self) -> String {
        let tail: Vec<char> = self.filter.chars().rev().take(FILTER_SHOWN).collect();
        let shown: String = tail.into_iter().rev().collect();
        format!("/{shown}▏")
    }

    fn hint(&self) -> &'static str {
        if !self.has_entries() {
            "↑↓ select · Enter open · n e s h q shortcuts · :help"
        } else if self.pending_recent {
            "R: press 1-0 to open that recent file"
        } else if self.filtering {
            "type to filter · Enter open · Tab column · Alt-p pin · Esc clear"
        } else if self.pinned.is_empty() {
            "R1-0 recent · j/k select · p pin · / filter · :help"
        } else {
            "1-9 A-K pinned · R1-0 recent · Tab column · p pin"
        }
    }

    fn fixed_rows(&self, show_logo: bool, show_extras: bool) -> usize {
        let compact = self.compact;
        let mut rows = (if compact { 1 } else { 3 }) + self.menu_len();
        if show_logo && !compact {
            rows += LOGO.len() + 1;
        }
        if show_extras {
            rows += if compact { 1 } else { 2 };
            if self.has_entries() {
                rows += if compact { 1 } else { 2 };
                if self.filtering {
                    rows += 1;
                }
            }
        }
        rows
    }

    fn column(&self, pinned: bool, col_w: usize, budget: usize) -> Vec<Cell> {
        let (list, filtered) = if pinned {
            (&self.pinned, &self.pinned_f)
        } else {
            (&self.recent, &self.recent_f)
        };
        let mut cells = vec![Cell::text(&self.heading(pinned), AnnotationType::Keyword, col_w)];
        if filtered.is_empty() {
            let note = if list.is_empty() { "(none)" } else { "(no matches)" };
            cells.push(Cell::text(note, AnnotationType::Punctuation, col_w));
            return cells;
        }
        let visible = budget.min(filtered.len());
        let is_active = self.active_pinned == pinned;
        let start = match self.selected {
            Some(i) if is_active && i >= visible => i + 1 - visible,
            _ => 0,
        };
        let max_label = col_w.saturating_sub(6);
        for (pos, &index) in filtered.iter().enumerate().skip(start).take(visible) {
            let Some(path) = list.get(index) else {
                continue;
            };
            let key = Self::key_label(pinned, pos);
            let icon = self.icon(if pinned { PIN_ICON } else { RECENT_ICON });
            let selected = is_active && self.selected == Some(pos);
            cells.push(self.cell(icon, &Self::label_for(path, max_label), &key, selected, col_w));
        }
        cells
    }

    fn push_entries(&self, rows: &mut Vec<AnnotatedString>, budget: usize) {
        let width = self.size.width;
        if self.filtering {
            rows.push(self.centered(&self.filter_text(), Some(AnnotationType::Keyword)));
        }
        let two_columns = !self.pinned.is_empty() && width >= 2 * MIN_COL + COL_GAP;
        if two_columns {
            let col_w = COL_W.min((width - COL_GAP) / 2);
            let pad = (width - (2 * col_w + COL_GAP)) / 2;
            let left = self.column_for(true, col_w, budget);
            let right = self.column_for(false, col_w, budget);
            for n in 0..left.len().max(right.len()) {
                let l = left.get(n).cloned().unwrap_or_else(|| Cell::blank(col_w));
                let r = right.get(n).cloned().unwrap_or_else(|| Cell::blank(col_w));
                rows.push(Self::compose(pad, vec![l, r]));
            }
        } else {
            // Recents alone (centered), or the active column when too narrow for two.
            let pinned = !self.pinned.is_empty() && self.active_pinned;
            for cell in self.column(pinned, MENU_WIDTH, budget) {
                rows.push(Self::compose(self.block_pad(), vec![cell]));
            }
        }
    }

    fn column_ws(&self, pinned: bool, col_w: usize, budget: usize) -> Vec<Cell> {
        let (list, filtered) = if pinned { (&self.pinned, &self.pinned_f) } else { (&self.recent, &self.recent_f) };
        let mut cells = vec![Cell::text(&self.heading(pinned), AnnotationType::Keyword, col_w)];
        if filtered.is_empty() {
            let note = if list.is_empty() { "(none)" } else { "(no matches)" };
            cells.push(Cell::text(note, AnnotationType::Punctuation, col_w));
            return cells;
        }
        let visible = budget.min(filtered.len());
        let is_active = self.active_pinned == pinned;
        let start = match self.selected { Some(i) if is_active && i >= visible => i + 1 - visible, _ => 0 };
        for (pos, &index) in filtered.iter().enumerate().skip(start).take(visible) {
            let Some(path) = list.get(index) else { continue };
            let p = std::path::Path::new(path);
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
            let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
            let selected = is_active && self.selected == Some(pos);
            let icon = self.icon(if pinned { PIN_ICON } else { RECENT_ICON });
            cells.push(self.cell(icon, name, &Self::key_label(pinned, pos), selected, col_w));
            let mut sub = Cell::text(&format!("   {}", Self::label_for(dir, col_w.saturating_sub(5))),
                                    AnnotationType::Comment, col_w);
            if selected { let n = sub.text.len(); sub.notes.push((AnnotationType::Selection, 0, n)); }
            cells.push(sub);
        }
        cells
    }

    fn assemble(&self, show_logo: bool, show_extras: bool, budget: usize) -> Vec<AnnotatedString> {
        let compact = self.compact;
        let mut rows = Vec::new();
        if show_logo && !compact {
            for line in LOGO {
                rows.push(self.centered(line, Some(AnnotationType::Attribute)));
            }
            rows.push(AnnotatedString::default());
        }
        if compact {
            rows.push(self.centered(&format!("vMACS {VERSION}"), Some(AnnotationType::Punctuation)));
        } else {
            rows.push(self.centered(
                "Versatile Modal And Command System",
                Some(AnnotationType::Punctuation),
            ));
            rows.push(self.centered(VERSION, Some(AnnotationType::Punctuation)));
            rows.push(AnnotatedString::default());
        }
        for (i, item) in self.menu().iter().enumerate() {
            rows.push(self.menu_row(
                self.icon(item.icon),
                &item.label,
                item.key,
                self.menu_sel == Some(i),
            ));
        }
        if show_extras && self.has_entries() {
            if !compact {
                rows.push(AnnotatedString::default());
            }
            self.push_entries(&mut rows, budget);
        }
        if show_extras {
            if !compact {
                rows.push(AnnotatedString::default());
            }
            rows.push(self.centered(self.hint(), Some(AnnotationType::Punctuation)));
        }
        rows
    }


    pub fn set_animate(&mut self, on: bool) {
        self.animate = on;
    }

    pub fn begin_open(&mut self) {
        if self.animate {
            self.open_anim = Some(Instant::now());
            self.set_needs_redraw(true);
        }
    }

    pub fn animating(&self) -> bool {
        self.open_anim.is_some() || self.sel_anim.is_some()
    }

    pub fn tick_animation(&mut self) -> bool {
        let mut live = false;
        if let Some(s) = self.open_anim {
            if s.elapsed().as_secs_f32() * 1000.0 >= OPEN_MS { self.open_anim = None; }
            live = true;
        }
        if let Some(s) = self.sel_anim {
            if s.elapsed().as_millis() >= 200 { self.sel_anim = None; }
            live = true;
        }
        if live { self.needs_redraw = true; }
        live
    }

}

impl UIComponent for Dashboard {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let height = self.size.height;
        let rows = if self.workstation {
            self.render_ws()
        } else {
            let logo_fits = self.size.width >= LOGO_WIDTH.saturating_add(2) && !self.compact;
            let mut found = None;
            for (logo, extras) in [(logo_fits, true), (false, true), (false, false)] {
                let budget = height.saturating_sub(self.fixed_rows(logo, extras)).min(MAX_VISIBLE);
                if extras && self.any_filtered() && budget == 0 {
                    continue;
                }
                let candidate = self.assemble(logo, extras, budget);
                if candidate.len() <= height {
                    found = Some(candidate);
                    break;
                }
            }
            found.unwrap_or_else(|| self.assemble(false, false, 0))
        };

        let stage = match self.sel_anim {
            Some(s) if s.elapsed().as_millis() < 90 => 1,
            Some(s) if s.elapsed().as_millis() < 200 => 2,
            _ => 0,
        };
        SEL_STAGE.store(stage, Ordering::Relaxed);
        let shown = match self.open_anim {
            Some(s) => {
                let p = (s.elapsed().as_secs_f32() * 1000.0 / OPEN_MS).min(1.0);
                let eased = 1.0 - (1.0 - p) * (1.0 - p);
                ((rows.len() as f32 * eased).ceil() as usize).min(rows.len())
            }
            None => rows.len(),
        };

        let top = height.saturating_sub(rows.len()) / 2;
        for offset in 0..height {
            let row = origin_row.saturating_add(offset);
            match offset
                .checked_sub(top)
                .filter(|&i| i < shown)
                .and_then(|index| rows.get(index))
            {
                Some(line) => Terminal::print_annotated_row(row, 0, line, None, false)?,
                None => Terminal::print_row(row, "")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dash() -> Dashboard {
        let mut d = Dashboard::default();
        d.set_entries(
            vec!["/p/pinned.rs".to_string()],
            vec![
                "/a/foo.rs".to_string(),
                "/a/bar.rs".to_string(),
                "/b/Foo.txt".to_string(),
                "/p/pinned.rs".to_string(),
            ],
        );
        d
    }

    fn press(d: &mut Dashboard, c: char) -> RecentKey {
        d.handle_key(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn tab(d: &mut Dashboard) {
        d.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    }

    #[test]
    fn pinned_are_not_repeated_in_recent() {
        let d = dash();
        assert_eq!(d.pinned, vec!["/p/pinned.rs"]);
        assert_eq!(d.recent, vec!["/a/foo.rs", "/a/bar.rs", "/b/Foo.txt"]);
    }

    #[test]
    fn plain_digits_open_pins() {
        let mut d = dash();
        assert_eq!(press(&mut d, '1'), RecentKey::Open("/p/pinned.rs".to_string()));
        assert_eq!(press(&mut d, '2'), RecentKey::Handled);
    }

    #[test]
    fn recents_open_with_r_prefix_or_shift_digit() {
        let mut d = dash();
        press(&mut d, 'r');
        assert_eq!(press(&mut d, '2'), RecentKey::Open("/a/bar.rs".to_string()));
        assert_eq!(
            d.handle_key(KeyCode::Char('!'), KeyModifiers::SHIFT),
            RecentKey::Open("/a/foo.rs".to_string())
        );
        assert_eq!(
            d.handle_key(KeyCode::Char('1'), KeyModifiers::SHIFT),
            RecentKey::Open("/a/foo.rs".to_string())
        );
    }

    #[test]
    fn r_prefix_is_dropped_by_other_keys() {
        let mut d = dash();
        press(&mut d, 'r');
        press(&mut d, 'j');
        assert_eq!(press(&mut d, '1'), RecentKey::Open("/p/pinned.rs".to_string()));
    }

    #[test]
    fn arrows_walk_the_menu_then_the_list() {
        let mut d = dash();
        let down = |d: &mut Dashboard| d.handle_key(KeyCode::Down, KeyModifiers::NONE);
        down(&mut d);
        assert_eq!(d.menu_sel, Some(0));
        assert_eq!(
            d.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            RecentKey::Menu('n')
        );
        let last = d.menu_len() - 1;
        for _ in 0..last {
            down(&mut d);
        }
        assert_eq!(d.menu_sel, Some(last));
        down(&mut d);
        assert_eq!((d.menu_sel, d.selected), (None, Some(0)));
        d.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(d.menu_sel, Some(last));
    }

    #[test]
    fn without_pins_digits_do_nothing_but_recents_still_open() {
        let mut d = Dashboard::default();
        d.set_entries(Vec::new(), vec!["/a/foo.rs".to_string()]);
        assert_eq!(press(&mut d, '1'), RecentKey::Handled);
        press(&mut d, 'r');
        assert_eq!(press(&mut d, '1'), RecentKey::Open("/a/foo.rs".to_string()));
    }

    #[test]
    fn filter_spans_both_columns() {
        let mut d = dash();
        press(&mut d, '/');
        for c in "foo".chars() {
            press(&mut d, c);
        }
        assert!(d.pinned_f.is_empty());
        assert_eq!(d.recent_f, vec![0, 2]);
        for c in " rs".chars() {
            press(&mut d, c);
        }
        assert_eq!(d.recent_f, vec![0]);
    }

    #[test]
    fn navigation_and_column_switching() {
        let mut d = dash();
        assert_eq!(press(&mut d, 'p'), RecentKey::Handled);
        tab(&mut d);
        assert_eq!(press(&mut d, 'p'), RecentKey::TogglePin("/a/foo.rs".to_string()));
        press(&mut d, 'j');
        assert_eq!(
            d.handle_key(KeyCode::Enter, KeyModifiers::NONE),
            RecentKey::Open("/a/bar.rs".to_string())
        );
        tab(&mut d);
        assert_eq!(press(&mut d, 'p'), RecentKey::TogglePin("/p/pinned.rs".to_string()));
    }

    #[test]
    fn alt_p_toggles_while_filtering() {
        let mut d = dash();
        press(&mut d, '/');
        assert_eq!(
            d.handle_key(KeyCode::Char('p'), KeyModifiers::ALT),
            RecentKey::TogglePin("/p/pinned.rs".to_string())
        );
        assert_eq!(press(&mut d, 'p'), RecentKey::Handled);
        assert_eq!(d.filter, "p");
    }

    #[test]
    fn selection_follows_the_file_when_it_is_pinned() {
        let mut d = dash();
        tab(&mut d);
        press(&mut d, 'j');
        assert_eq!(d.selected_path().as_deref(), Some("/a/bar.rs"));
        d.set_entries(
            vec!["/p/pinned.rs".to_string(), "/a/bar.rs".to_string()],
            vec![
                "/a/foo.rs".to_string(),
                "/a/bar.rs".to_string(),
                "/b/Foo.txt".to_string(),
            ],
        );
        assert!(d.active_pinned);
        assert_eq!(d.selected, Some(1));
    }

    #[test]
    fn menu_keys_fall_through_when_not_filtering() {
        let mut d = dash();
        assert_eq!(press(&mut d, 'q'), RecentKey::Ignored);
        press(&mut d, '/');
        assert_eq!(press(&mut d, 'q'), RecentKey::Handled);
        assert_eq!(d.filter, "q");
    }

    #[test]
    fn removing_keeps_selection_in_range() {
        let mut d = dash();
        tab(&mut d);
        press(&mut d, 'j');
        press(&mut d, 'j');
        d.remove_entry("/b/Foo.txt");
        assert_eq!(d.selected, Some(1));
        for path in ["/a/foo.rs", "/a/bar.rs", "/p/pinned.rs"] {
            d.remove_entry(path);
        }
        assert_eq!(d.selected, None);
    }
}