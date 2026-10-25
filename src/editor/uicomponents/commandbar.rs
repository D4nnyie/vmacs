#![allow(clippy::arithmetic_side_effects, clippy::integer_division)]

use std::{cmp::min, collections::HashMap, fs, io::Error, path::PathBuf};

use std::time::Instant;

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

use crate::prelude::*;

use super::super::{command::Edit, Line, Terminal};
use super::UIComponent;
use super::filebrowser::expand_tilde;
use super::super::theme as t;

const BOX_BG: Color = t::SURFACE_RAISED; const BOX_BORDER: Color = t::BLUE; const BOX_PROMPT: Color = t::GREEN;
const BOX_TEXT: Color = t::TEXT_BRIGHT; const LIST_SELECTED_BG: Color = t::SURFACE_ACTIVE;
const LIST_DESC: Color = t::TEXT_MUTED;

const LIST_ANIM_MS: u128 = 110;
const LIST_NAME_W: usize = 10;
const MAX_LIST_ROWS: usize = 12;
const BOX_ANIM_MS: u128 = 120;

/// (name, argument hint, description)
const COMMANDS: &[(&str, &str, &str)] = &[
    ("w", "[file]", "save"),
    ("wq", "[file]", "save and quit"),
    ("x", "", "save and quit"),
    ("q", "", "quit"),
    ("q!", "", "quit, discard edits"),
    ("e", "[path]", "open (browse if empty)"),
    ("e!", "file", "open, discard edits"),
    ("undo", "", "undo (:u)"),
    ("redo", "", "redo (:red)"),
    ("s", "/a/b/g", "substitute (prefix a range)"),
    ("qr", "", "query replace (M-%)"),
    ("sort", "[n r u i]", "sort lines (prefix a range)"),
    ("fold", "", "toggle the fold at the cursor"),
    ("foldall", "", "close all folds"),
    ("unfoldall", "", "open all folds"),
    ("w!", "[file]", "save, overwriting outside changes"),
    ("yanks", "[clear]", "yank history (pick to paste)"),
    ("goto", "[line[:col]]", "go to line / column (M-g)"),
    ("col", "N", "go to column"),
    ("tabnew", "[file]", "new buffer"),
    ("bnext", "", "next buffer (:bn)"),
    ("bprev", "", "previous buffer (:bp)"),
    ("bdelete", "", "close buffer (:bd!)"),
    ("buffer", "N", "go to buffer N"),
    ("ls", "", "list buffers"),
    ("wa", "", "save all buffers"),
    ("wqa", "", "save all and quit"),
    ("session", "", "restore the last session"),
    ("noh", "", "clear search highlight"),
    ("number", "", "line numbers (:nu)"),
    ("path", "", "toggle the full path in the status bar"),
    ("ui", "[ws|full|compact]", "interface mode"),
    ("layout", "[center|left|right]", "dashboard layout"),
    ("jumps", "", "show the jump list"),
    ("encoding", "[name]", "show / set the save encoding"),
    ("eol", "[lf|crlf|cr]", "show / set line endings"),
    ("reopen", "enc", "reload the file with an encoding"),
    ("readonly", "", "toggle read-only (:ro)"),
    ("configdir", "", "open the config folder"),
    ("newlang", "[name]", "new language syntax file"),
    ("autopairs", "", "auto-pairs (:ap)"),
    ("config", "", "open the settings file"),
    ("reload", "", "reload the settings file"),
    ("keys", "", "your keybinds and commands"),
    ("dashboard", "", "back to the dashboard (:dash)"),
    ("dashboard!", "", "same, discard edits"),
    ("pin", "[file]", "pin a file to the dashboard"),
    ("unpin", "[file]", "unpin a file"),
    ("help", "", "help screen (:h)"),
    ("version", "", "Prints out the vMACS version (:ver)"),
];

const PATH_COMMANDS: &[&str] = &["e", "e!", "edit", "edit!", "w", "write", "wq", "x", "pin", "unpin", "tabnew", "tabe", "tabedit"];

#[derive(Default)]
pub struct CommandBar {
    prompt: String,
    title: String,
    hint: String,
    value: Line,
    needs_redraw: bool,
    size: Size,
    suggest: bool,
    browsing: bool,
    selected: usize,
    path_browsing: bool,
    path_items: Vec<(String, bool)>,
    path_head: String,
    path_base: String,
    custom: Option<Vec<(String, String)>>,
    history: Vec<String>,
    hist_pos: Option<usize>,
    draft: String,
    usage: HashMap<String, usize>,
    order: Vec<usize>,
    top: usize,
    animate: bool,
    list_open: bool,
    list_anim: Option<Instant>,
    animate_box: bool,
    box_anim: Option<Instant>,
}

impl CommandBar {
    pub fn handle_edit_command(&mut self, command: Edit) {
        match command {
            Edit::Insert(character) => {
                self.hist_pos = None;
                self.value.append_char(character);
                self.browsing = false;
                self.path_browsing = false;
            }
            Edit::DeleteBackward => {
                self.hist_pos = None;
                self.value.delete_last();
                self.browsing = false;
                self.path_browsing = false;
            }
            Edit::Delete | Edit::InsertNewline | Edit::InsertIndent => {}
            Edit::DeleteLine | Edit::DeleteToLineEnd => {}
            Edit::DeleteCharYank | Edit::YankLine | Edit::Paste | Edit::PasteBefore | Edit::PasteInline => {}
            Edit::DeleteSelection | Edit::YankSelection | Edit::YankSelectionSystem | Edit::UppercaseSelection
            | Edit::LowercaseSelection | Edit::IndentSelection | Edit::DedentSelection | Edit::Undo | Edit::Redo  | Edit::Operate { .. } | Edit::JoinLines(_) | Edit::ToggleCase(_) | Edit::ReplaceChar(..) | Edit::Repeat => {}
        }
        self.set_needs_redraw(true);
    }
    pub fn set_top(&mut self, top: usize) {
        self.top = top;
    }
    pub fn caret_position_col(&self) -> ColIdx {
        let max_width = self.prompt.width().saturating_add(self.value.width());
        min(max_width, self.size.width)
    }
    pub fn value(&self) -> String {
        self.value.to_string()
    }
    pub fn set_prompt(&mut self, prompt: &str) {
        self.prompt = prompt.to_string();
        self.set_needs_redraw(true);
    }

    pub fn set_chrome(&mut self, title: &str, hint: &str) {
        self.title = title.to_string();
        self.hint = hint.to_string();
        self.set_needs_redraw(true);
    }
    pub fn clear_value(&mut self) {
        self.hist_pos = None;
        self.draft.clear();
        self.value = Line::default();
        self.browsing = false;
        self.path_browsing = false;
        self.selected = 0;
        self.set_needs_redraw(true);
    }

    pub fn set_suggestions(&mut self, on: bool) {
        self.suggest = on;
        self.browsing = false;
        self.path_browsing = false;
    }

    pub fn set_animate(&mut self, on: bool) {
        self.animate = on;
    }

    fn note_list(&mut self) {
        let open = !self.visible().is_empty() || self.path_browsing || self.path_context().is_some();
        if open && !self.list_open && self.animate {
            self.list_anim = Some(Instant::now());
        }
        if !open {
            self.list_anim = None;
        }
        self.list_open = open;
    }

    fn box_growing(&self) -> bool {
        self.box_anim.is_some_and(|t| t.elapsed().as_millis() < BOX_ANIM_MS)
    }

    pub fn animating(&self) -> bool {
        self.list_anim.is_some_and(|t| t.elapsed().as_millis() < LIST_ANIM_MS) || self.box_growing()
    }

    pub fn tick_animation(&mut self) -> bool {
        if self.list_anim.is_none() && self.box_anim.is_none() {
            return false;
        }
        if !self.list_anim.is_some_and(|t| t.elapsed().as_millis() < LIST_ANIM_MS) {
            self.list_anim = None;
        }
        if !self.box_growing() {
            self.box_anim = None;
        }
        self.needs_redraw = true;
        true
    }

    pub fn set_animate_box(&mut self, on: bool) {
        self.animate_box = on;
        if !on {
            self.box_anim = None;
        }
    }

    pub fn begin_open(&mut self) {
        self.box_anim = self.animate_box.then(Instant::now);
    }

    /// Box geometry for this frame: the full size, or a narrower centered box while it grows.
    #[allow(clippy::as_conversions, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn live_geometry(&self, area: Size) -> Option<(Position, usize)> {
        let (origin, full) = Self::geometry(area)?;
        let Some(start) = self.box_anim.filter(|_| self.box_growing()) else {
            return Some((origin, full));
        };
        let p = (start.elapsed().as_secs_f32() * 1000.0 / 120.0).min(1.0);
        let eased = 1.0 - (1.0 - p).powi(3);
        let w = (((full as f32) * (0.4 + 0.6 * eased)).round() as usize).clamp(24.min(full), full);
        Some((Position { col: origin.col + (full - w) / 2, row: origin.row }, w))
    }

    #[allow(clippy::as_conversions, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn reveal(&self, rows: usize) -> usize {
        let Some(start) = self.list_anim else {
            return rows;
        };
        let p = (start.elapsed().as_secs_f32() * 1000.0 / 110.0).min(1.0);
        let eased = 1.0 - (1.0 - p) * (1.0 - p);
        ((rows as f32 * eased).ceil() as usize).clamp(1, rows.max(1))
    }

    pub fn cycle_suggestion(&mut self, forward: bool) {
        if !self.suggest {
            return;
        }
        if self.path_browsing {
            self.cycle_path(forward);
            return;
        }
        if let Some((head, base, prefix)) = self.path_context() {
            self.start_path_completion(head, base, prefix);
            return;
        }
        let typed = self.value.to_string();
        if typed.contains(char::is_whitespace) {
            return;
        }
        let len = self.order.len();
        if len == 0 {
            return;
        }
        if self.browsing {
            let pos = self.order.iter().position(|&i| i == self.selected).unwrap_or(0);
            let next = if forward { (pos + 1) % len } else { (pos + len - 1) % len };
            self.selected = self.order[next];
        } else {
            let Some(first) = self
                .order
                .iter()
                .copied()
                .find(|&i| self.entry(i).0.starts_with(typed.as_str()))
            else {
                return;
            };
            self.browsing = true;
            self.selected = first;
        }
        let chosen = self.entry(self.selected).0.to_string();
        self.value = Line::from(&chosen);
        self.set_needs_redraw(true);
    }

    pub fn accept_suggestion(&mut self) {
        if self.browsing || self.path_browsing {
            self.browsing = false;
            self.path_browsing = false;
            self.set_needs_redraw(true);
        }
    }

    fn path_context(&self) -> Option<(String, String, String)> {
        if !self.suggest || self.custom.is_some() {
            return None;
        }
        let typed = self.value.to_string();
        let split = typed.find(char::is_whitespace)?;
        let cmd = &typed[..split];
        if !PATH_COMMANDS.iter().any(|candidate| *candidate == cmd) {
            return None;
        }
        let arg = typed[split..].trim_start();
        let head = &typed[..typed.len() - arg.len()];
        let (base, prefix) = if arg == "~" {
            ("~/".to_string(), String::new())
        } else {
            let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
            match arg.rfind(separators) {
                Some(index) => (arg[..=index].to_string(), arg[index + 1..].to_string()),
                None => (String::new(), arg.to_string()),
            }
        };
        Some((head.to_string(), base, prefix))
    }

    fn list_dir(base: &str, prefix: &str) -> Vec<(String, bool)> {
        let dir = if base.is_empty() { PathBuf::from(".") } else { expand_tilde(base) };
        let Ok(read) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let show_hidden = prefix.starts_with('.');
        let prefix_lower = prefix.to_lowercase();
        let mut items: Vec<(String, bool)> = read
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.to_lowercase().starts_with(&prefix_lower)
                    || (!show_hidden && name.starts_with('.'))
                {
                    return None;
                }
                let is_dir = fs::metadata(entry.path()).is_ok_and(|meta| meta.is_dir());
                Some((name, is_dir))
            })
            .collect();
        items.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        items
    }

    fn common_prefix(items: &[(String, bool)]) -> String {
        let mut names = items.iter().map(|(name, _)| name.as_str());
        let Some(first) = names.next() else {
            return String::new();
        };
        let mut len = first.len();
        for name in names {
            let shared: usize = first
                .chars()
                .zip(name.chars())
                .take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
                .map(|(a, _)| a.len_utf8())
                .sum();
            len = len.min(shared);
        }
        first[..len].to_string()
    }

    fn fill_path(&mut self, name: &str, is_dir: bool) {
        let mut text = format!("{}{}{}", self.path_head, self.path_base, name);
        if is_dir {
            text.push('/');
        }
        self.value = Line::from(&text);
    }

    fn start_path_completion(&mut self, head: String, base: String, prefix: String) {
        let items = Self::list_dir(&base, &prefix);
        let Some((first_name, first_is_dir)) = items.first().cloned() else {
            return;
        };
        self.path_head = head;
        self.path_base = base;
        if items.len() == 1 {
            self.fill_path(&first_name, first_is_dir);
        } else {
            let common = Self::common_prefix(&items);
            if common.len() > prefix.len() {
                self.fill_path(&common, false);
            } else {
                self.path_items = items;
                self.selected = 0;
                self.path_browsing = true;
                self.fill_path(&first_name, first_is_dir);
            }
        }
        self.set_needs_redraw(true);
    }

    fn cycle_path(&mut self, forward: bool) {
        let len = self.path_items.len();
        if len == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else {
            (self.selected + len - 1) % len
        };
        let (name, is_dir) = self.path_items[self.selected].clone();
        self.fill_path(&name, is_dir);
        self.set_needs_redraw(true);
    }

    fn draw_path_list(&self, origin: Position, box_w: usize, area: Size) -> Result<(), Error> {
        let top = origin.row + 3;
        let rows = area.height.saturating_sub(top).min(MAX_LIST_ROWS);
        if rows == 0 {
            return Ok(());
        }
        let live: Vec<(String, bool)>;
        let items: &[(String, bool)] = if self.path_browsing {
            &self.path_items
        } else {
            let Some((_, base, prefix)) = self.path_context() else {
                return Ok(());
            };
            live = Self::list_dir(&base, &prefix);
            &live
        };
        let shown = self.reveal(rows.min(items.len()));
        if items.is_empty() {
            return Ok(());
        }
        let start = if self.path_browsing {
            (self.selected + 1).saturating_sub(rows)
        } else {
            0
        };
        let room = box_w.saturating_sub(4);
        for (n, (name, is_dir)) in items.iter().enumerate().skip(start).take(shown) {
            let selected = self.path_browsing && n == self.selected;
            let bg = if selected { LIST_SELECTED_BG } else { BOX_BG };
            let row = top + (n - start);
            let at = |c: usize| Position { row, col: c };
            Terminal::print_styled_at(at(origin.col), &" ".repeat(box_w), BOX_TEXT, bg, false)?;
            let label = if *is_dir { format!("{name}/") } else { name.clone() };
            let label: String = label.chars().take(room).collect();
            let fg = if *is_dir { BOX_BORDER } else { BOX_TEXT };
            Terminal::print_styled_at(at(origin.col + 2), &label, fg, bg, selected)?;
        }
        Ok(())
    }

    fn visible(&self) -> Vec<usize> {
        if !self.suggest {
            return Vec::new();
        }
        let typed = self.value.to_string();
        if typed.contains(char::is_whitespace) {
            return Vec::new();
        }
        if self.browsing {
            return self.order.clone();
        }
        if typed.is_empty() {
            return Vec::new();
        }
        self.order
            .iter()
            .copied()
            .filter(|&i| self.entry(i).0.starts_with(typed.as_str()))
            .collect()
    }

    fn draw_suggestions(&self, origin: Position, box_w: usize, area: Size) -> Result<(), Error> {
        let list = self.visible();
        let top = origin.row + 3;
        let rows = area.height.saturating_sub(top).min(MAX_LIST_ROWS);
        let shown = self.reveal(rows.min(list.len()));
        if list.is_empty() || rows == 0 {
            return Ok(());
        }
        let start = if self.browsing {
            list.iter()
                .position(|&index| index == self.selected)
                .map_or(0, |pos| (pos + 1).saturating_sub(rows))
        } else {
            0
        };
        let col = origin.col;

        const MIN_DESC: usize = 14;
        let widest = list
            .iter()
            .skip(start)
            .take(rows)
            .map(|&i| self.entry(i).1.chars().count())
            .max()
            .unwrap_or(0);
        let max_args = box_w.saturating_sub(2 + LIST_NAME_W + 1 + MIN_DESC);
        let args_w = if widest == 0 { 0 } else { widest.saturating_add(1).min(max_args) };
        let desc_col = col + 2 + LIST_NAME_W + args_w;
        let room = box_w.saturating_sub(desc_col - col + 1);

        for (n, &index) in list.iter().skip(start).take(shown).enumerate() {
            let (name, args, desc) = self.entry(index);
            let name: String = name.chars().take(LIST_NAME_W - 1).collect();
            let bg = if self.browsing && index == self.selected { LIST_SELECTED_BG } else { BOX_BG };
            let row = top + n;
            let at = |c: usize| Position { row, col: c };
            Terminal::print_styled_at(at(col), &" ".repeat(box_w), BOX_TEXT, bg, false)?;
            Terminal::print_styled_at(at(col + 2), &name, BOX_PROMPT, bg, true)?;

            if args_w > 0 && !args.is_empty() {
                let limit = args_w.saturating_sub(1);
                let shown: String = if args.chars().count() > limit {
                    let mut s: String = args.chars().take(limit.saturating_sub(1)).collect();
                    s.push('…');
                    s
                } else {
                    args.to_string()
                };
                Terminal::print_styled_at(at(col + 2 + LIST_NAME_W), &shown, t::PURPLE, bg, false)?;
            }

            let desc: String = desc.chars().take(room).collect();
            Terminal::print_styled_at(at(desc_col), &desc, LIST_DESC, bg, false)?;
        }
        Ok(())
    }

    fn geometry(area: Size) -> Option<(Position, usize)> {
        if area.height < 5 || area.width < 20 {
            return None;
        }
        let box_w = (area.width * 3 / 5).clamp(40, 72).min(area.width);
        let col = (area.width - box_w) / 2;
        let row = area.height.saturating_sub(3) / 2;
        Some((Position { col, row }, box_w))
    }

    pub fn fits_centered(area: Size) -> bool {
        Self::geometry(area).is_some()
    }

    pub fn caret_position_centered(&self, area: Size) -> Option<Position> {
        let (origin, box_w) = self.live_geometry(area)?;
        let inner = box_w - 4;
        let prompt_w = self.prompt.width();
        let value_area = inner.saturating_sub(prompt_w);
        Some(Position {
            row: origin.row + 1 + self.top,
            col: origin.col + 2 + prompt_w + min(self.value.width(), value_area),
        })
    }

    pub fn render_centered(&mut self, area: Size) {
        self.note_list();
        if !self.needs_redraw {
            return;
        }
        let _ = self.draw_centered(area);
        self.needs_redraw = false;
    }

    fn border_line(left: char, right: char, label: &str, box_w: usize) -> String {
        let inner = box_w.saturating_sub(2);
        let label = if label.is_empty() { String::new() } else { format!(" {label} ") };
        let dashes = "─".repeat(inner.saturating_sub(1 + label.width()));
        format!("{left}─{label}{dashes}{right}")
    }

    fn draw_centered(&self, area: Size) -> Result<(), Error> {
        let Some((origin, box_w)) = self.live_geometry(area) else {
            return Ok(());
        };
        let origin = Position { col: origin.col, row: origin.row + self.top };
        let area = Size { height: area.height + self.top, width: area.width };
        let (row, col) = (origin.row, origin.col);
        let inner = box_w - 4;
        let prompt_w = self.prompt.width();
        let value_area = inner.saturating_sub(prompt_w);
        let value_end = self.value.width();
        let value_start = value_end.saturating_sub(value_area);
        let visible = self.value.get_visible_graphemes(value_start..value_end);
        let visible_w = visible.width();
        let pad = inner.saturating_sub(prompt_w + visible_w);

        let top = Self::border_line('╭', '╮', &self.title, box_w);
        let bottom = Self::border_line('╰', '╯', &self.hint, box_w);
        Terminal::print_styled_at(Position { row, col }, &top, BOX_BORDER, BOX_BG, false)?;

        let input_row = row + 1;
        let at = |c: usize| Position { row: input_row, col: c };
        Terminal::print_styled_at(at(col), "│ ", BOX_BORDER, BOX_BG, false)?;
        Terminal::print_styled_at(at(col + 2), &self.prompt, BOX_PROMPT, BOX_BG, true)?;
        Terminal::print_styled_at(at(col + 2 + prompt_w), &visible, BOX_TEXT, BOX_BG, false)?;
        Terminal::print_styled_at(
            at(col + 2 + prompt_w + visible_w),
            &" ".repeat(pad),
            BOX_TEXT,
            BOX_BG,
            false,
        )?;
        Terminal::print_styled_at(at(col + box_w - 2), " │", BOX_BORDER, BOX_BG, false)?;

        Terminal::print_styled_at(
            Position { row: row + 2, col },
            &bottom,
            BOX_BORDER,
            BOX_BG,
            false,
        )?;
        if self.path_browsing || self.path_context().is_some() {
            self.draw_path_list(origin, box_w, area)
        } else {
            self.draw_suggestions(origin, box_w, area)
        }
    }

    pub fn set_custom_entries(&mut self, entries: Option<Vec<(String, String)>>) {
        self.custom = entries;
        self.rebuild_order();
    }

    fn entry_count(&self) -> usize {
        self.custom.as_ref().map_or(COMMANDS.len(), Vec::len)
    }

    fn entry(&self, index: usize) -> (&str, &str, &str) {
        match &self.custom {
            Some(list) => list
                .get(index)
                .map_or(("", "", ""), |(name, desc)| (name.as_str(), "", desc.as_str())),
            None => COMMANDS[index],
        }
    }

    pub fn set_history(&mut self, history: Vec<String>, usage: HashMap<String, usize>) {
        self.history = history;
        self.usage = usage;
        self.hist_pos = None;
        self.rebuild_order();
    }

    fn rebuild_order(&mut self) {
        let mut order: Vec<usize> = (0..self.entry_count()).collect();
        order.sort_by_key(|&i| {
            std::cmp::Reverse(self.usage.get(self.entry(i).0).copied().unwrap_or(0))
        });
        self.order = order;
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.hist_pos {
            None => {
                self.draft = self.value.to_string();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(pos) => pos - 1,
        };
        self.show_history(pos);
    }

    pub fn history_next(&mut self) {
        let Some(pos) = self.hist_pos else {
            return;
        };
        if pos + 1 < self.history.len() {
            self.show_history(pos + 1);
        } else {
            self.hist_pos = None;
            self.browsing = false;
            self.path_browsing = false;
            let draft = std::mem::take(&mut self.draft);
            self.value = Line::from(&draft);
            self.set_needs_redraw(true);
        }
    }

    fn show_history(&mut self, pos: usize) {
        self.hist_pos = Some(pos);
        self.browsing = false;
        self.path_browsing = false;
        let text = self.history[pos].clone();
        self.value = Line::from(&text);
        self.set_needs_redraw(true);
    }
}

impl UIComponent for CommandBar {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }
    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    fn set_size(&mut self, size: Size) {
        self.size = size;
    }
    fn draw(&mut self, origin: RowIdx) -> Result<(), Error> {
        let area_for_value = self.size.width.saturating_sub(self.prompt.width());
        let value_end = self.value.width();
        let value_start = value_end.saturating_sub(area_for_value);
        let message = format!(
            "{}{}",
            self.prompt,
            self.value.get_visible_graphemes(value_start..value_end)
        );
        let to_print = if message.width() <= self.size.width {
            message
        } else {
            String::new()
        };
        Terminal::print_row(origin, &to_print)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn typed(text: &str) -> CommandBar {
        let mut bar = CommandBar::default();
        bar.set_suggestions(true);
        for c in text.chars() {
            bar.handle_edit_command(Edit::Insert(c));
        }
        bar
    }

    fn names(items: Vec<(String, bool)>) -> Vec<String> {
        items.into_iter().map(|(name, _)| name).collect()
    }

    fn sample_dir() -> TempDir {
        let dir = TempDir::new();
        dir.write("README.md", b"");
        dir.write("readme_old.txt", b"");
        dir.write(".hidden", b"");
        std::fs::create_dir(dir.path().join("Src")).unwrap();
        dir
    }

    #[test]
    fn path_commands_include_write() {
        for cmd in ["e", "e!", "edit", "w", "write", "wq", "x"] {
            let bar = typed(&format!("{cmd} /tmp/foo/ba"));
            assert_eq!(
                bar.path_context(),
                Some((format!("{cmd} "), "/tmp/foo/".to_string(), "ba".to_string())),
                "{cmd}"
            );
        }
        assert!(typed("q /tmp/foo").path_context().is_none());
        assert!(typed("w").path_context().is_none());
    }

    #[test]
    fn listing_is_case_insensitive_and_sorted() {
        let dir = sample_dir();
        let base = format!("{}/", dir.path().display());
        assert_eq!(names(CommandBar::list_dir(&base, "re")), vec!["README.md", "readme_old.txt"]);
        assert_eq!(names(CommandBar::list_dir(&base, "RE")), vec!["README.md", "readme_old.txt"]);

        let src = CommandBar::list_dir(&base, "s");
        assert_eq!(src, vec![("Src".to_string(), true)]);
    }

    #[test]
    fn hidden_files_need_a_dot() {
        let dir = sample_dir();
        let base = format!("{}/", dir.path().display());
        assert!(!names(CommandBar::list_dir(&base, "")).contains(&".hidden".to_string()));
        assert_eq!(names(CommandBar::list_dir(&base, ".h")), vec![".hidden"]);
    }

    #[test]
    fn folders_come_first() {
        let dir = sample_dir();
        let base = format!("{}/", dir.path().display());
        assert_eq!(names(CommandBar::list_dir(&base, ""))[0], "Src");
    }

    #[test]
    fn common_prefix_ignores_case() {
        let items = vec![("README.md".to_string(), false), ("readme_old.txt".to_string(), false)];
        assert_eq!(CommandBar::common_prefix(&items), "README");
        assert_eq!(CommandBar::common_prefix(&[]), "");
    }

    #[test]
    fn tab_completes_then_cycles() {
        let dir = sample_dir();
        let base = format!("{}/", dir.path().display());
        let mut bar = typed(&format!("w {base}re"));

        bar.cycle_suggestion(true);
        assert_eq!(bar.value(), format!("w {base}README"));
        bar.cycle_suggestion(true);
        assert_eq!(bar.value(), format!("w {base}README.md"));
        bar.cycle_suggestion(true);
        assert_eq!(bar.value(), format!("w {base}readme_old.txt"));
        bar.cycle_suggestion(false);
        assert_eq!(bar.value(), format!("w {base}README.md"));
    }

    #[test]
    fn single_match_completes_fully_and_folders_get_a_slash() {
        let dir = sample_dir();
        let base = format!("{}/", dir.path().display());
        let mut bar = typed(&format!("e {base}sr"));
        bar.cycle_suggestion(true);
        assert_eq!(bar.value(), format!("e {base}Src/"));
    }
}