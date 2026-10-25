use std::{cmp::min, io::Error};

use crate::editor::RowIdx;
use crate::prelude::*;
use crossterm::style::Color;

use super::super::{
    command::{Edit, FoldOp, Move, Op, Target},
    DocumentStatus, Line, Terminal,
};
use super::super::ex::{replace_in_line, SortArgs, Substitute};
use super::UIComponent;
mod buffer;
use buffer::{Buffer, Candidate, Encoding};
mod searchdirection;
use searchdirection::SearchDirection;
mod register;
use register::Register;
mod highlighter;
use highlighter::Highlighter;
pub use highlighter::{highlight_lines, new_language_file};
mod fileinfo;
use fileinfo::FileInfo;
mod searchinfo;
use searchinfo::SearchInfo;
mod tscache;
mod folds;
use unicode_segmentation::UnicodeSegmentation;
use super::super::yankhistory::YankHistory;
use super::super::theme as t;
use std::time::Instant;
const GUIDE_ANIM_MS: u128 = 200;
const POPUP_ANIM_MS: u128 = 100;

const FILL_MS: f32 = 220.0;

#[derive(Clone, Copy)]
pub enum BigEdit {
    Deleted(usize),
    Pasted(usize),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ClickUnit {
    #[default]
    Char,
    Word,
    Line,
}

/// 0 = word characters, 1 = whitespace, 2 = punctuation.
fn grapheme_class(g: &str) -> u8 {
    if g.chars().all(|c| c.is_alphanumeric() || c == '_') {
        0
    } else if g.chars().all(char::is_whitespace) {
        1
    } else {
        2
    }
}

struct Completion {
    items: Vec<Candidate>,
    selected: usize,
    start: GraphemeIdx,
}

const MAX_JUMPS: usize = 100;
const H_STEP: isize = 4;

fn display_path(path: &std::path::Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .filter(|home| !home.as_os_str().is_empty());
    if let Some(home) = home {
        if let Ok(rest) = abs.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    abs.display().to_string()
}

fn unknown_encoding(name: &str) -> String {
    format!("E474: Unknown encoding: {name} ({})", Encoding::NAMES)
}

#[derive(Default)]
pub struct View {
    buffer: Buffer,
    needs_redraw: bool,
    size: Size,
    text_location: Location,
    scroll_offset: Position,
    search_info: Option<SearchInfo>,
    register: Register,
    selection_anchor: Option<Location>,
    no_autopairs: bool,
    no_line_numbers: bool,
    last_search: Option<String>,
    search_backward: bool,
    hl_search: bool,
    click_unit: ClickUnit,
    click_span: Option<(Location, Location)>,
    scope_guide: bool,
    scope_highlight: bool,
    scope_brackets: bool,
    yanks: YankHistory,
    block_cursor: bool,
    selection_inclusive: bool,
    goal_col: Option<(Location, ColIdx)>,
    blocked_edit: bool,
    jumps: Vec<Location>,
    jump_pos: usize,
    want_insert: bool,
    notice: Option<String>,
    animations: bool,
    minimal_gutter: bool,
    shown_row: RowIdx,
    guide_key: Option<(usize, LineIdx)>,
    guide_anim: Option<Instant>,
    compact: bool,
    shown_col: ColIdx,
    h_free: bool,
    trail: Option<Position>,
    completion: Option<Completion>,
    comp_rev: u64,
    popup_animation: bool,
    comp_anim: Option<Instant>,
    fill_anim: Option<Instant>,
    fold_hover: Option<usize>,
    big_edit_min: usize,
    big_edit: Option<BigEdit>,
}

const SCROLL_TRACK: Color = t::LINE_DIM; const SCROLL_THUMB: Color = t::BLUE_MUTED;
const INDENT_GUIDE: Color = t::GUIDE; const INDENT_GUIDE_ACTIVE: Color = t::BLUE;
const INDENT_GUIDE_OUTER: Color = t::BLUE_MUTED; const FOLD_FG: Color = t::TEXT_DIM;
const FOLD_BG: Color = t::SURFACE_FOLD; const FOLD_OPEN: Color = t::TEXT_SOFT; const FOLD_CLOSED: Color = t::BLUE_SOFT;
const FOLD_HOVER_BG: Color = t::SURFACE_ACTIVE;

#[derive(Clone)]
struct Guides {
    unit: usize,
    active: Vec<(usize, LineIdx, LineIdx)>,
    reveal: Option<(LineIdx, usize)>,
}

impl View {

    fn visible_height(&self) -> usize {
        self.buffer
            .height()
            .saturating_sub(self.buffer.hidden_map().count_hidden())
    }

    pub fn set_popup_animation(&mut self, on: bool) {
        self.popup_animation = on;
    }

    fn popup_animates(&self) -> bool {
        self.popup_animation && self.animate()
    }

    fn ensure_cursor_visible(&mut self) {
        let hidden = self.buffer.hidden_final();
        if let Some((s, _)) = hidden.range_containing(self.text_location.line_idx) {
            self.text_location.line_idx = s.saturating_sub(1);
            self.snap_to_valid_grapheme();
        }
    }

    fn cursor_gliding(&self) -> bool {
        match (self.trail, self.caret_position()) {
            (Some(c), Some(t)) => c.row != t.row || c.col != t.col,
            _ => false,
        }
    }

    fn tick_cursor(&mut self) -> bool {
        let Some(target) = self.caret_position() else {
            return self.trail.take().is_some();
        };
        let Some(cur) = self.trail else {
            self.trail = Some(target);
            return false;
        };
        if cur.row == target.row && cur.col == target.col {
            return false;
        }
        let dist = cur.row.abs_diff(target.row) + cur.col.abs_diff(target.col);
        let scrolling = self.shown_row != self.scroll_offset.row
            || self.shown_col != self.scroll_offset.col;
        if scrolling || dist < 3 {
            self.trail = Some(target);
            return dist >= 3;
        }
        self.trail = Some(Position {
            row: Self::chase(cur.row, target.row),
            col: Self::chase(cur.col, target.col),
        });
        true
    }

    pub fn completion_open(&self) -> bool {
        self.completion.is_some()
    }

    pub fn completion_count(&self) -> usize {
        self.completion.as_ref().map_or(0, |c| c.items.len())
    }

    pub fn close_completion(&mut self) {
        self.comp_anim = None;
        if self.completion.take().is_some() {
            self.set_needs_redraw(true);
        }
    }

    pub fn completion_move(&mut self, down: bool) {
        let Some(c) = self.completion.as_mut() else {
            return;
        };
        let n = c.items.len().max(1);
        c.selected = if down { (c.selected + 1) % n } else { (c.selected + n - 1) % n };
        self.set_needs_redraw(true);
    }

    /// `force` (Tab) ignores the minimum length and the "text unchanged" check.
    pub fn refresh_completion(&mut self, min_chars: usize, max_items: usize, force: bool) {
        let rev = self.buffer.revision();
        if !force && rev == self.comp_rev {
            self.close_completion(); // cursor moved without typing
            return;
        }
        self.comp_rev = rev;
        let at = self.text_location;
        let found = self
            .buffer
            .word_before(at)
            .filter(|(prefix, _)| force || prefix.chars().count() >= min_chars);
        let Some((prefix, start)) = found else {
            self.close_completion();
            return;
        };
        let items = self.buffer.completions(at, &prefix, start, max_items.max(1));
        if items.is_empty() {
            self.close_completion();
            return;
        }
        let keep = self
            .completion
            .as_ref()
            .and_then(|c| c.items.get(c.selected))
            .map(|item| item.text.clone());
        let selected = keep
            .and_then(|k| items.iter().position(|i| i.text == k))
            .unwrap_or(0);
        let was_open = self.completion.is_some();
        self.completion = Some(Completion { items, selected, start });
        if !was_open && self.popup_animates() {
            self.comp_anim = Some(Instant::now());
        }
        self.set_needs_redraw(true);
    }

    pub fn accept_completion(&mut self) -> bool {
        if self.guard_read_only() {
            return false;
        }
        let Some(c) = self.completion.take() else {
            return false;
        };
        let Some(item) = c.items.get(c.selected) else {
            return false;
        };
        let word = item.text.clone();
        let at = self.text_location;
        let from = Location { line_idx: at.line_idx, grapheme_idx: c.start };
        self.buffer.mark_cursor(at);
        self.buffer.delete_range(from, at);
        self.text_location = self.buffer.insert_str(from, &word);
        self.comp_rev = self.buffer.revision();
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
        true
    }

    fn draw_completion(&self, origin_row: RowIdx) -> Result<(), Error> {
        let Some(c) = &self.completion else {
            return Ok(());
        };
        let Some(caret) = self.caret_position() else {
            return Ok(());
        };
        let line = self.text_location.line_idx;
        let prefix_cols = self
            .buffer
            .width_until(line, self.text_location.grapheme_idx)
            .saturating_sub(self.buffer.width_until(line, c.start));
        let widest = c.items.iter().map(|i| i.text.chars().count() + i.kind.len()).max().unwrap_or(0);
        let w = widest.saturating_add(5).clamp(16, 44).min(self.size.width);
        let x = caret.col.saturating_sub(prefix_cols).min(self.size.width.saturating_sub(w));
        let n = c.items.len().min(self.size.height);
        let below = caret.row.saturating_add(1).saturating_add(n) <= self.size.height;
        let row0 = if below { caret.row.saturating_add(1) } else { caret.row.saturating_sub(n) };
        let plen = self.text_location.grapheme_idx.saturating_sub(c.start);

        #[allow(clippy::as_conversions, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let shown = match self.comp_anim {
            Some(start) => {
                let p = (start.elapsed().as_secs_f32() * 1000.0 / 100.0).min(1.0);
                let eased = 1.0 - (1.0 - p) * (1.0 - p);
                (((n as f32) * eased).ceil() as usize).clamp(1, n.max(1))
            }
            None => n,
        };
        let skip = if below { 0 } else { n.saturating_sub(shown) };

        for (i, item) in c.items.iter().take(n).enumerate().skip(skip).take(shown) {
            let selected = i == c.selected;
            let bg = if selected { t::SURFACE_ACTIVE } else { t::SURFACE_RAISED };
            let row = origin_row.saturating_add(row0).saturating_add(i);
            let at = |col: usize| Position { row, col };
            Terminal::print_styled_at(at(x), &" ".repeat(w), t::TEXT, bg, false)?;
            if selected {
                Terminal::print_styled_at(at(x), "▌", t::BLUE, bg, false)?; // not color-only
            }
            let room = w.saturating_sub(3 + item.kind.len());
            let text: String = item.text.chars().take(room).collect();
            let head: String = text.chars().take(plen).collect();
            let tail: String = text.chars().skip(plen).collect();
            Terminal::print_styled_at(at(x + 1), &head, t::BLUE, bg, true)?;
            let fg = if selected { t::TEXT_BRIGHT } else { t::TEXT };
            Terminal::print_styled_at(at(x + 1 + head.chars().count()), &tail, fg, bg, selected)?;
            Terminal::print_styled_at(at(x + w - 1 - item.kind.len()), item.kind, t::TEXT_DIM, bg, false)?;
        }
        Ok(())
    }

    pub fn ghost_cursor(&self) -> Option<Position> {
        if !self.animate() {
            return None;
        }
        let (target, cur) = (self.caret_position()?, self.trail?);
        let dist = cur.row.abs_diff(target.row) + cur.col.abs_diff(target.col);
        (dist >= 3).then_some(cur)
    }

    pub fn fold_command(&mut self, op: FoldOp) -> bool {
        self.buffer.set_animate_folds(self.animate());
        let line = self.text_location.line_idx;
        let done = match op {
            FoldOp::Toggle => self.buffer.fold_toggle(line),
            FoldOp::Open => self.buffer.fold_open(line),
            FoldOp::Close => self.buffer.fold_close(line),
            FoldOp::OpenAll => {
                self.buffer.fold_open_all();
                true
            }
            FoldOp::CloseAll => self.buffer.fold_close_all(),
            FoldOp::Next => self.buffer.fold_next_start(line).map(|t| self.go_to_fold_line(t)).is_some(),
            FoldOp::Prev => self.buffer.fold_prev_end(line).map(|t| self.go_to_fold_line(t)).is_some(),
        };
        self.ensure_cursor_visible();
        self.clamp_block();
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        done
    }

    fn go_to_fold_line(&mut self, line_idx: LineIdx) {
        self.text_location = Location {
            line_idx,
            grapheme_idx: self.buffer.first_non_blank(line_idx),
        };
    }

    pub fn get_status(&self) -> DocumentStatus {
        let file_info = self.buffer.get_file_info();
        DocumentStatus {
            total_lines: self.buffer.height(),
            current_line_idx: self.text_location.line_idx,
            current_column: self.text_location.grapheme_idx,
            file_name: format!("{file_info}"),
            is_modified: self.buffer.is_dirty(),
            file_type: file_info.file_type_name(),
            encoding: self.buffer.encoding_label(),
            file_path: file_info.get_path().map(display_path).unwrap_or_default(),
            read_only: self.buffer.is_read_only(),
            line_ending: self.buffer.line_ending_label(),
            indent: self.buffer.indent_style(),
        }
    }

    pub const fn is_file_loaded(&self) -> bool {
        self.buffer.is_file_loaded()
    }

    pub fn language_name(&self) -> String {
        self.buffer.get_file_info().file_type_name()
    }

    pub fn enter_search(&mut self) {
        self.search_info = Some(SearchInfo {
            prev_location: self.text_location,
            prev_scroll_offset: self.scroll_offset,
            query: None,
        });
    }
    pub fn exit_search(&mut self) {
        self.search_info = None;
        self.set_needs_redraw(true);
    }
    pub fn dismiss_search(&mut self) {
        if let Some(search_info) = &self.search_info {
            self.text_location = search_info.prev_location;
            self.scroll_offset = search_info.prev_scroll_offset;
            self.scroll_text_location_into_view();
        }
        self.exit_search();
    }

    pub fn search(&mut self, query: &str) {
        if let Some(search_info) = &mut self.search_info {
            search_info.query = Some(Line::from(query));
        }
        self.search_in_direction(self.text_location, SearchDirection::default());
    }

    fn get_search_query(&self) -> Option<&Line> {
        self.search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_ref())
    }

    fn search_in_direction(&mut self, from: Location, direction: SearchDirection) {
        if let Some(location) = self.get_search_query().and_then(|query| {
            if query.is_empty() {
                None
            } else if direction == SearchDirection::Forward {
                self.buffer.search_forward(query, from)
            } else {
                self.buffer.search_backward(query, from)
            }
        }) {
            self.text_location = location;
            self.buffer.fold_reveal(self.text_location.line_idx);
            self.center_text_location();
        };
        self.set_needs_redraw(true);
    }
    pub fn search_next(&mut self) {
        let step_right = self
            .get_search_query()
            .map_or(1, |query| min(query.grapheme_count(), 1));

        let location = Location {
            line_idx: self.text_location.line_idx,
            grapheme_idx: self.text_location.grapheme_idx.saturating_add(step_right),
        };
        self.search_in_direction(location, SearchDirection::Forward);
    }
    pub fn search_prev(&mut self) {
        self.search_in_direction(self.text_location, SearchDirection::Backward);
    }

    pub fn load(&mut self, file_name: &str) -> Result<(), Error> {
        let buffer = Buffer::load(file_name)?;
        self.buffer = buffer;
        self.text_location = Location::default();
        self.scroll_offset = Position::default();
        self.selection_anchor = None;
        self.search_info = None;
        self.shown_col = 0;
        self.shown_row = 0;
        self.set_needs_redraw(true);
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.save_forced(false)
    }

    pub fn save_forced(&mut self, force: bool) -> Result<(), Error> {
        self.seal_undo_group();
        self.buffer.save_with(force)?;
        self.set_needs_redraw(true);
        Ok(())
    }
    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        self.seal_undo_group();
        self.buffer.save_as(file_name)?;
        self.set_needs_redraw(true);
        Ok(())
    }

    pub fn handle_edit_command(&mut self, command: Edit) {
        if self.buffer.is_read_only()
            && !matches!(
                command,
                Edit::YankLine
                    | Edit::YankSelection
                    | Edit::YankSelectionSystem
                    | Edit::Undo
                    | Edit::Redo
                    | Edit::Operate { op: Op::Yank, .. }
            )
        {
            self.blocked_edit = true;
            return;
        }
        if !matches!(
            command,
            Edit::YankLine | Edit::YankSelection | Edit::YankSelectionSystem
                | Edit::Undo | Edit::Redo | Edit::Operate { op: Op::Yank, .. }
        ) {
        }
        if self.h_free {
            self.scroll_text_location_into_view();
        }
        self.buffer.mark_cursor(self.text_location);
        match command {
            Edit::Insert(character) => self.insert_char(character),
            Edit::Delete => self.delete(),
            Edit::DeleteBackward => self.delete_backward(),
            Edit::InsertNewline => self.insert_newline(),
            Edit::DeleteLine => self.delete_line(),
            Edit::DeleteToLineEnd => self.delete_to_line_end(),
            Edit::DeleteCharYank => self.delete_char_yank(),
            Edit::YankLine => self.yank_line(),
            Edit::Paste => self.paste(false),
            Edit::PasteBefore => self.paste(true),
            Edit::PasteInline => self.paste_inline(),
            Edit::DeleteSelection => self.delete_selection(),
            Edit::YankSelection => self.yank_selection(),
            Edit::YankSelectionSystem => {
                if let Some(text) = self.yank_selection_to_system_clipboard() {
                    self.yanks.record(&text);
                    super::super::clipboard::copy_to_system_clipboard(&text);
                }
                self.clear_selection();
            }
            Edit::UppercaseSelection => self.change_case_selection(true),
            Edit::LowercaseSelection => self.change_case_selection(false),
            Edit::IndentSelection => self.indent_selection(true),
            Edit::DedentSelection => self.indent_selection(false),
            Edit::InsertIndent => self.insert_indent(),
            Edit::Operate { op, target, count } => self.apply_operator(op, target, count),
            Edit::JoinLines(n) => self.join_lines(n),
            Edit::ToggleCase(n) => self.toggle_case(n),
            Edit::ReplaceChar(c, n) => self.replace_char(c, n),
            Edit::Repeat => {}
            Edit::Undo => {
                self.try_undo();
            }
            Edit::Redo => {
                self.try_redo();
            }
        }
    }
    pub fn handle_move_command(&mut self, command: Move) {
        if !command.is_viewport() {
            self.clear_selection();
        }
        self.move_cursor(command);
        self.set_needs_redraw(true);
    }

    pub fn handle_select_command(&mut self, command: Move) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.text_location);
            self.selection_inclusive = self.block_cursor;
        }
        let before = self.text_location;
        self.move_cursor(command);
        if command.is_inclusive() && !self.selection_inclusive {
            let now = self.text_location;
            if (now.line_idx, now.grapheme_idx) > (before.line_idx, before.grapheme_idx) {
                let count = self.buffer.grapheme_count(now.line_idx);
                self.text_location.grapheme_idx = now.grapheme_idx.saturating_add(1).min(count);
                self.scroll_text_location_into_view();
            }
        }
        self.set_needs_redraw(true);
    }

    fn move_cursor(&mut self, command: Move) {
        let Size { height, .. } = self.size;
        let jump_from = command.is_jump().then_some(self.text_location);
        let vertical = matches!(
            command,
            Move::Up
                | Move::Down
                | Move::PageUp
                | Move::PageDown
                | Move::HalfPageUp
                | Move::HalfPageDown
                | Move::ViewUp
                | Move::ViewDown
        );
        let goal = vertical.then(|| self.goal_column());
        match command {
            Move::Up => self.move_up(1),
            Move::Down => self.move_down(1),
            Move::Left if self.block_cursor => self.move_left_in_line(),
            Move::Left => self.move_left(),
            Move::Right if self.block_cursor => self.move_right_in_line(),
            Move::Right => self.move_right(),
            Move::PageUp => self.move_up(height.saturating_sub(1)),
            Move::PageDown => self.move_down(height.saturating_sub(1)),
            Move::StartOfLine => self.move_to_start_of_line(),
            Move::EndOfLine => self.move_to_end_of_line(),
            Move::JumpBack => self.jump_back(),
            Move::JumpForward => self.jump_forward(),
            Move::WordForward => {
                self.text_location = self.buffer.find_next_word_start(self.text_location);
            }
            Move::WordBackward => {
                self.text_location = self.buffer.find_prev_word_start(self.text_location);
            }
            Move::ParagraphForward => {
                self.text_location = self.buffer.find_next_paragraph(self.text_location);
            }
            Move::ParagraphBackward => {
                self.text_location = self.buffer.find_prev_paragraph(self.text_location);
            }
            Move::BufferStart => {
                self.text_location = Location { line_idx: 0, grapheme_idx: 0 };
            }
            Move::BufferEnd => {
                self.text_location.line_idx = self.buffer.height().saturating_sub(1);
                self.snap_to_valid_grapheme();
            }
            Move::WordEnd => {
                self.text_location = self.buffer.find_word_end(self.text_location);
            }
            Move::BigWordForward => {
                self.text_location = self.buffer.find_next_bigword_start(self.text_location);
            }
            Move::BigWordBackward => {
                self.text_location = self.buffer.find_prev_bigword_start(self.text_location);
            }
            Move::BigWordEnd => {
                self.text_location = self.buffer.find_bigword_end(self.text_location);
            }
            Move::FirstNonBlank => {
                self.text_location.grapheme_idx =
                    self.buffer.first_non_blank(self.text_location.line_idx);
            }
            Move::LastNonBlank => {
                self.text_location.grapheme_idx =
                    self.buffer.last_non_blank(self.text_location.line_idx);
            }
            Move::MatchingBracket => {
                if let Some(target) = self.buffer.matching_bracket(self.text_location) {
                    self.text_location = target;
                }
            }
            Move::Find { ch, forward, till } => {
                if let Some(target) =
                    self.buffer.find_char_in_line(self.text_location, ch, forward, till)
                {
                    self.text_location = target;
                }
            }
            Move::SmartHome => {
                let indent = self.buffer.indent_end(self.text_location.line_idx);
                self.text_location.grapheme_idx = if self.text_location.grapheme_idx == indent {
                    0
                } else {
                    indent
                };
            }
            Move::ScreenTop | Move::ScreenMiddle | Move::ScreenBottom => {
                let line_idx = self.screen_line(command);
                self.text_location.line_idx = line_idx;
                self.text_location.grapheme_idx = self.buffer.first_non_blank(line_idx);
            }
            Move::ViewUp => self.scroll_viewport(false, 1),
            Move::ViewDown => self.scroll_viewport(true, 1),
            Move::ViewTop => {
                self.scroll_offset.row = self.text_location_to_position().row;
            }
            Move::ViewCenter => {
                self.scroll_offset.row = self
                    .text_location_to_position()
                    .row
                    .saturating_sub(self.size.height.div_ceil(2));
            }
            Move::ViewBottom => {
                self.scroll_offset.row = self
                    .text_location_to_position()
                    .row
                    .saturating_add(1)
                    .saturating_sub(self.size.height);
            }
            Move::HalfPageUp => self.half_page(false),
            Move::HalfPageDown => self.half_page(true),
            Move::ViewLeft => self.scroll_viewport_h(-1),
            Move::ViewRight => self.scroll_viewport_h(1),
            Move::ViewHalfLeft => {
                let n = self.half_width();
                self.scroll_viewport_h(-n);
            }
            Move::ViewHalfRight => {
                let n = self.half_width();
                self.scroll_viewport_h(n);
            }
            Move::ViewColStart => self.scroll_offset.col = self.cursor_col(),
            Move::ViewColEnd => {
                self.scroll_offset.col = self.cursor_col().saturating_add(1).saturating_sub(self.text_width());
            }
            Move::NextLineStart => {
                self.move_down(1);
                self.text_location.grapheme_idx = self.buffer.first_non_blank(self.text_location.line_idx);
            }
            Move::PrevLineStart => {
                self.move_up(1);
                self.text_location.grapheme_idx = self.buffer.first_non_blank(self.text_location.line_idx);
            }
        }
        if let Some(col) = goal {
            self.apply_goal_col(col);
        }
        self.ensure_cursor_visible();
        self.clamp_block();
        if let Some(col) = goal {
            self.goal_col = Some((self.text_location, col));
        }
        if let Some(from) = jump_from {
            self.note_jump(from);
        }
        self.scroll_text_location_into_view();
    }


    #[allow(clippy::integer_division)]
    fn screen_line(&self, kind: Move) -> LineIdx {
        let last_v = self.visible_height().saturating_sub(1);
        let top = self.scroll_offset.row.min(last_v);
        let bottom = self
            .scroll_offset
            .row
            .saturating_add(self.size.height.saturating_sub(1))
            .min(last_v);
        let row = match kind {
            Move::ScreenTop => top,
            Move::ScreenMiddle => top.saturating_add(bottom.saturating_sub(top) / 2),
            _ => bottom,
        };
        self.buffer.hidden_map().line_of_vrow(row)
    }

    fn scroll_viewport(&mut self, down: bool, step: usize) {
        let max_row = self.visible_height().saturating_sub(1);
        self.scroll_offset.row = if down {
            self.scroll_offset.row.saturating_add(step).min(max_row)
        } else {
            self.scroll_offset.row.saturating_sub(step)
        };
        self.clamp_cursor_to_view();
    }

    fn clamp_cursor_to_view(&mut self) {
        if self.size.height == 0 {
            return;
        }
        let top = self.scroll_offset.row;
        let bottom = top.saturating_add(self.size.height.saturating_sub(1));
        let row = self.text_location_to_position().row.clamp(top, bottom);
        self.text_location.line_idx = self
            .buffer
            .hidden_map()
            .line_of_vrow(row)
            .min(self.buffer.height().saturating_sub(1));
        self.snap_to_valid_grapheme();
    }

    fn delete_line(&mut self) {
        if let Some(content) = self.buffer.line_content(self.text_location.line_idx) {
            self.register = Register { content, linewise: true };
        }
        self.buffer.delete_line(self.text_location.line_idx);
        self.snap_to_valid_line();
        self.move_to_start_of_line();
        self.set_needs_redraw(true);
    }
    fn delete_char_yank(&mut self) {
        if let Some(g) = self.buffer.grapheme_at(self.text_location) {
            self.register = Register { content: g, linewise: false };
        }
        self.delete();
    }

    pub fn start_selection(&mut self) {
        self.selection_anchor = Some(self.text_location);
        self.selection_inclusive = self.block_cursor;
        self.set_needs_redraw(true);
    }

    pub fn clear_selection(&mut self) {
        if self.selection_anchor.take().is_some() {
            self.set_needs_redraw(true);
        }
    }

    fn selection_range(&self) -> Option<(Location, Location)> {
        self.selection_anchor.map(|anchor| {
            let anchor = self.buffer.clamp_location(anchor);
            let cursor = self.buffer.clamp_location(self.text_location);
            let a = (anchor.line_idx, anchor.grapheme_idx);
            let c = (cursor.line_idx, cursor.grapheme_idx);
            let (start, end) = if a <= c { (anchor, cursor) } else { (cursor, anchor) };
            if self.selection_inclusive {
                // Vim-style: the character under the cursor is part of the selection.
                let count = self.buffer.grapheme_count(end.line_idx);
                let grapheme_idx = end.grapheme_idx.saturating_add(1).min(count);
                (start, Location { line_idx: end.line_idx, grapheme_idx })
            } else {
                (start, end)
            }
        })
    }

    pub fn paste_text(&mut self, text: &str) {
        if self.guard_read_only() {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
        self.note_big(BigEdit::Pasted(text.matches('\n').count().saturating_add(1)));
        self.seal_undo_group();
        self.buffer.mark_cursor(self.text_location);
        if self.has_selection() {
            self.delete_selection();
        } else {
            self.clear_selection();
        }
        self.text_location = self.buffer.insert_str(self.text_location, &text);
        self.seal_undo_group();
        self.set_needs_redraw(true);
        self.normalize_cursor();
        self.scroll_text_location_into_view();
    }

    fn selection_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        if start.line_idx == end.line_idx {
            self.buffer.substring(start.line_idx, start.grapheme_idx..end.grapheme_idx)
        } else {
            let mut result = String::new();
            for line_idx in start.line_idx..=end.line_idx {
                let range = if line_idx == start.line_idx {
                    start.grapheme_idx..self.buffer.grapheme_count(line_idx)
                } else if line_idx == end.line_idx {
                    0..end.grapheme_idx
                } else {
                    0..self.buffer.grapheme_count(line_idx)
                };
                if let Some(s) = self.buffer.substring(line_idx, range) {
                    result.push_str(&s);
                }
                if line_idx != end.line_idx {
                    result.push('\n');
                }
            }
            Some(result)
        }
    }

    fn insert_text_at(&mut self, at: Location, text: &str) -> Location {
        if text.is_empty() {
            return at;
        }
        self.buffer.insert_str(at, text)
    }

    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            if let Some(text) = self.selection_text() {
                self.register = Register { content: text, linewise: false };
            }
            self.note_big(BigEdit::Deleted(end.line_idx.saturating_sub(start.line_idx).saturating_add(1)));
            self.buffer.delete_range(start, end);
            self.text_location = start;
            self.clear_selection();
            self.set_needs_redraw(true);
        }
    }

    pub fn replace_selection_for_typing(&mut self) {
        if !self.has_selection() {
            return;
        }
        self.buffer.mark_cursor(self.text_location);
        let saved = self.register.clone();
        self.delete_selection();
        self.register = saved;
    }

    pub fn yank_selection_to_system_clipboard(&self) -> Option<String> {
        self.selection_text()
    } 

    fn yank_selection(&mut self) {
        if let Some((start, _)) = self.selection_range() {
            if let Some(text) = self.selection_text() {
                self.yanks.record(&text);
                self.register = Register { content: text, linewise: false };
            }
            self.text_location = start;
            self.clear_selection();
            self.set_needs_redraw(true);
        }
    }

    fn change_case_selection(&mut self, upper: bool) {
        if let Some((start, end)) = self.selection_range() {
            if let Some(text) = self.selection_text() {
                let changed = if upper { text.to_uppercase() } else { text.to_lowercase() };
                self.buffer.delete_range(start, end);
                self.text_location = self.insert_text_at(start, &changed);
            }
            self.set_needs_redraw(true);
            self.clear_selection();
        }
    }

    fn indent_selection(&mut self, indent: bool) {
        if let Some((start, end)) = self.selection_range() {
            for line_idx in start.line_idx..=end.line_idx {
                if indent {
                    for _ in INDENT.chars() {
                        self.buffer.insert_char(' ', Location { line_idx, grapheme_idx: 0 });
                    }
                    self.shift_line_markers(line_idx, INDENT.len(), true);
                } else {
                    let removed = self.buffer.delete_leading_tab_or_spaces(line_idx);
                    if removed > 0 {
                        self.shift_line_markers(line_idx, removed, false);
                    }
                }
            }
            self.set_needs_redraw(true);
        }
    }

    fn shift_line_markers(&mut self, line_idx: LineIdx, amount: GraphemeIdx, added: bool) {
        if let Some(anchor) = self.selection_anchor.as_mut() {
            if anchor.line_idx == line_idx {
                anchor.grapheme_idx = if added {
                    anchor.grapheme_idx.saturating_add(amount)
                } else {
                    anchor.grapheme_idx.saturating_sub(amount)
                };
            }
        }
        if self.text_location.line_idx == line_idx {
            self.text_location.grapheme_idx = if added {
                self.text_location.grapheme_idx.saturating_add(amount)
            } else {
                self.text_location.grapheme_idx.saturating_sub(amount)
            };
        }
    }

    fn yank_line(&mut self) {
        if let Some(content) = self.buffer.line_content(self.text_location.line_idx) {
            self.yanks.record(&content);
            self.register = Register { content, linewise: true };
        }
    }

    fn paste(&mut self, before: bool) {
        if self.register.content.is_empty() {
            return;
        }
        if self.register.linewise {
            let insert_idx = if before {
                self.text_location.line_idx
            } else {
                self.text_location.line_idx.saturating_add(1)
            };
            let content = self.register.content.clone();
            for (i, part) in content.split('\n').enumerate() {
                self.buffer.insert_line(insert_idx.saturating_add(i), part);
            }
            self.text_location = Location {
                line_idx: insert_idx,
                grapheme_idx: self.buffer.first_non_blank(insert_idx),
            };
        } else {
            let mut at = self.text_location;
            if !before {
                at.grapheme_idx = min(at.grapheme_idx.saturating_add(1), self.buffer.grapheme_count(at.line_idx));
            }
            let content = self.register.content.clone();
            self.text_location = self.insert_text_at(at, &content);
            if self.block_cursor && !content.contains('\n') {
                self.text_location.grapheme_idx = self.text_location.grapheme_idx.saturating_sub(1);
            }
        }
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn paste_inline(&mut self) {
        if self.register.content.is_empty() {
            return;
        }
        let content = self.register.content.clone();
        self.text_location = self.insert_text_at(self.text_location, &content);
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn delete_to_line_end(&mut self) {
        self.buffer.delete_to_line_end(self.text_location);
        self.set_needs_redraw(true);
    }
    fn insert_indent(&mut self) {
        if self.has_selection() {
            self.indent_selection(true);
            return;
        }
        let pad = INDENT.len() - (self.text_location.grapheme_idx % INDENT.len());
        let spaces = " ".repeat(pad);
        self.text_location = self.insert_text_at(self.text_location, &spaces);
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn insert_newline(&mut self) {
        self.clear_selection();
        let at = self.text_location;
        let between = self.buffer.autoindent_enabled() && self.buffer.is_between_pair(at);
        let level = self.buffer.newline_indent_level(at);

        self.buffer.insert_newline(at);
        let new_line = Location {
            line_idx: at.line_idx.saturating_add(1),
            grapheme_idx: 0,
        };
        if between {
            self.buffer.insert_newline(new_line);
            let closer_line = Location {
                line_idx: new_line.line_idx.saturating_add(1),
                grapheme_idx: 0,
            };
            self.insert_text_at(closer_line, &INDENT.repeat(level.saturating_sub(1)));
        }
        self.text_location = self.insert_text_at(new_line, &INDENT.repeat(level));
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn delete_backward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        self.clear_selection();
        if !self.no_autopairs && self.buffer.is_between_autopair(self.text_location) {
            self.buffer.delete(self.text_location);
        }
        if self.text_location.line_idx != 0 || self.text_location.grapheme_idx != 0 {
            self.move_left();
            self.scroll_text_location_into_view();
            self.delete();
        }
    }
    fn delete(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        self.clear_selection();
        self.buffer.delete(self.text_location);
        self.set_needs_redraw(true);
    }
    fn insert_char(&mut self, character: char) {
        if self.handle_autopair(character) {
            return;
        }
        let old_len = self.buffer.grapheme_count(self.text_location.line_idx);
        self.buffer.insert_char(character, self.text_location);
        let new_len = self.buffer.grapheme_count(self.text_location.line_idx);
        let grapheme_delta = new_len.saturating_sub(old_len);
        if grapheme_delta > 0 {
            self.handle_move_command(Move::Right);
            if matches!(character, '}' | ')' | ']') {
                self.auto_dedent(character);
            }
        }
        self.set_needs_redraw(true);
    }

    fn auto_dedent(&mut self, closer: char) {
        if !self.buffer.autoindent_enabled() {
            return;
        }
        let line_idx = self.text_location.line_idx;
        let closer_idx = self.text_location.grapheme_idx.saturating_sub(1);
        let Some(before) = self.buffer.substring(line_idx, 0..closer_idx) else {
            return;
        };
        if !before.chars().all(char::is_whitespace) {
            return;
        }
        let at = Location { line_idx, grapheme_idx: closer_idx };
        let Some(target) = self.buffer.matching_open_indent(at, closer) else {
            return;
        };
        if before == target {
            return;
        }
        if let Some((old, new)) = self.buffer.set_leading_whitespace(line_idx, &target) {
            self.text_location.grapheme_idx = self
                .text_location
                .grapheme_idx
                .saturating_sub(old)
                .saturating_add(new);
        }
    }
    pub fn open_line_above(&mut self) {
        if self.guard_read_only() {
            return;
        }
        self.buffer.mark_cursor(self.text_location);
        self.clear_selection();
        let line_idx = self.text_location.line_idx.min(self.buffer.height());
        let whitespace = INDENT.repeat(self.buffer.indent_level_for_open_above(line_idx));
        self.buffer.insert_line(line_idx, &whitespace);
        self.text_location = Location {
            line_idx,
            grapheme_idx: whitespace.chars().count(),
        };
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }

    fn render_line(at: RowIdx, line_text: &str) -> Result<(), Error> {
        Terminal::print_row(at, line_text)
    }
    #[allow(clippy::integer_division)]
    fn build_welcome_message(width: usize) -> String {
        let message = format!(" "); // was {NAME} editor -- version {VERSION}
        if width < message.len() {
            return String::new();
        }
        let left_padding = width.saturating_sub(message.len()) / 2;
        format!("{}{message}", " ".repeat(left_padding))
    }

    fn scroll_vertically(&mut self, to: RowIdx) {
        let Size { height, .. } = self.size;
        let offset_changed = if to < self.scroll_offset.row {
            self.scroll_offset.row = to;
            true
        } else if to >= self.scroll_offset.row.saturating_add(height) {
            self.scroll_offset.row = to.saturating_sub(height).saturating_add(1);
            true
        } else {
            false
        };
        if offset_changed {
            self.set_needs_redraw(true);
        }
    }
    fn scroll_horizontally(&mut self, to: ColIdx) {
        let width = self.text_width();
        let offset_changed = if to < self.scroll_offset.col {
            self.scroll_offset.col = to;
            true
        } else if to >= self.scroll_offset.col.saturating_add(width) {
            self.scroll_offset.col = to.saturating_sub(width).saturating_add(1);
            true
        } else {
            false
        };
        if offset_changed {
            self.set_needs_redraw(true);
        }
    }

    

    fn scroll_text_location_into_view(&mut self) {
        self.h_free = false;
        let Position { row, col } = self.text_location_to_position();
        self.scroll_vertically(row);
        self.scroll_horizontally(col);
    }
    fn center_text_location(&mut self) {
        let height = self.size.height; 
        let width = self.text_width();
        let Position { row, col } = self.text_location_to_position();
        let vertical_mid = height.div_ceil(2);
        let horizontal_mid = width.div_ceil(2);
        self.scroll_offset.row = row.saturating_sub(vertical_mid);
        self.scroll_offset.col = col.saturating_sub(horizontal_mid);
        self.set_needs_redraw(true);
    }

    pub fn caret_position(&self) -> Option<Position> {
        let pos = self.text_location_to_position();
        let row = pos.row.checked_sub(self.shown_row)?;
        let col = pos.col.checked_sub(self.shown_col)?;
        (row < self.size.height && col < self.text_width()).then_some(Position {
            row,
            col: col.saturating_add(self.gutter_width()),
        })
    }

    fn cursor_col(&self) -> ColIdx {
        self.buffer.width_until(self.text_location.line_idx, self.text_location.grapheme_idx)
    }

    fn max_left(&self) -> usize {
        let hidden = self.buffer.hidden_map();
        let top = self.scroll_offset.row;
        let mut widest = 0;
        for vrow in top..top.saturating_add(self.size.height) {
            let line = hidden.line_of_vrow(vrow);
            if line >= self.buffer.height() {
                break;
            }
            widest = widest.max(self.buffer.width_until(line, self.buffer.grapheme_count(line)));
        }
        widest.saturating_sub(1)
    }

    fn scroll_view_h(&mut self, delta: isize) {
        let cur = self.scroll_offset.col;
        let amount = delta.unsigned_abs();
        self.scroll_offset.col = if delta < 0 {
            cur.saturating_sub(amount)
        } else {
            cur.saturating_add(amount).min(self.max_left().max(cur))
        };
        self.h_free = true;
        self.set_needs_redraw(true);
    }

    pub fn scroll_view_fast(&mut self, down: bool) {
        let step = self.size.height.div_ceil(2).max(1);
        let max_row = self.visible_height().saturating_sub(1);
        self.fold_hover = None;
        self.scroll_offset.row = if down {
            self.scroll_offset.row.saturating_add(step).min(max_row)
        } else {
            self.scroll_offset.row.saturating_sub(step)
        };
        self.set_needs_redraw(true);
    }

    fn scroll_viewport_h(&mut self, delta: isize) {
        self.scroll_view_h(delta);
        self.clamp_cursor_to_cols();
    }

    fn clamp_cursor_to_cols(&mut self) {
        let left = self.scroll_offset.col;
        let right = left.saturating_add(self.text_width()).saturating_sub(1);
        let line = self.text_location.line_idx;
        if line >= self.buffer.height() {
            return;
        }
        let col = self.cursor_col();
        let target = if col < left { left } else if col > right { right } else { return };
        self.text_location.grapheme_idx = self.buffer.location_for_position(line, target).grapheme_idx;
        self.snap_to_valid_grapheme();
    }

    fn half_width(&self) -> isize {
        isize::try_from(self.text_width().div_ceil(2)).unwrap_or(1).max(1)
    }

    pub fn scroll_view_left(&mut self) { self.scroll_view_h(-H_STEP); }
    pub fn scroll_view_right(&mut self) { self.scroll_view_h(H_STEP); }

    fn text_location_to_position(&self) -> Position {
        let row = self.buffer.hidden_map().vrow(self.text_location.line_idx);
        debug_assert!(row.saturating_sub(1) <= self.buffer.height());
        let col = self.buffer.width_until(
            self.text_location.line_idx,
            self.text_location.grapheme_idx,
        );
        Position { col, row }
    }

    fn move_up(&mut self, step: usize) {
        let hidden = self.buffer.hidden_map();
        for _ in 0..step {
            let Some(mut line) = self.text_location.line_idx.checked_sub(1) else {
                break;
            };
            if let Some((s, _)) = hidden.range_containing(line) {
                line = s.saturating_sub(1);
            }
            self.text_location.line_idx = line;
        }
        self.snap_to_valid_grapheme();
    }

    fn move_down(&mut self, step: usize) {
        let hidden = self.buffer.hidden_map();
        for _ in 0..step {
            let mut line = self.text_location.line_idx.saturating_add(1);
            if let Some((_, e)) = hidden.range_containing(line) {
                line = e.saturating_add(1);
            }
            self.text_location.line_idx = line;
            if line >= self.buffer.height() {
                break;
            }
        }
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }

    fn goal_column(&self) -> ColIdx {
        match self.goal_col {
            Some((at, col))
                if at.line_idx == self.text_location.line_idx
                    && at.grapheme_idx == self.text_location.grapheme_idx =>
            {
                col
            }
            _ => self.text_location_to_position().col,
        }
    }

    fn apply_goal_col(&mut self, col: ColIdx) {
        let line_idx = self.text_location.line_idx;
        if line_idx < self.buffer.height() {
            self.text_location.grapheme_idx =
                self.buffer.location_for_position(line_idx, col).grapheme_idx;
        }
    }

    pub fn move_to_indent_for_insert(&mut self) {
        self.clear_selection();
        self.text_location.grapheme_idx =
            self.buffer.first_non_blank_or_end(self.text_location.line_idx);
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
    }

    fn half_page(&mut self, down: bool) {
        let step = self.size.height.div_ceil(2).max(1);
        let max_scroll = self.visible_height().saturating_sub(self.size.height);
        if down {
            self.scroll_offset.row = self.scroll_offset.row.saturating_add(step).min(max_scroll);
            self.move_down(step);
            self.text_location.line_idx = self
                .text_location
                .line_idx
                .min(self.buffer.height().saturating_sub(1));
            self.snap_to_valid_grapheme();
        } else {
            self.scroll_offset.row = self.scroll_offset.row.saturating_sub(step);
            self.move_up(step);
        }
    }

    /*
    #[allow(clippy::integer_division)]
    fn screen_line(&self, kind: Move) -> LineIdx {
        let last_line = self.buffer.height().saturating_sub(1);
        let top = self.scroll_offset.row.min(last_line);
        let bottom = self
            .scroll_offset
            .row
            .saturating_add(self.size.height.saturating_sub(1))
            .min(last_line);
        match kind {
            Move::ScreenTop => top,
            Move::ScreenMiddle => top.saturating_add(bottom.saturating_sub(top) / 2),
            _ => bottom,
        }
    }

    fn scroll_viewport(&mut self, down: bool, step: usize) {
        let max_row = self.buffer.height().saturating_sub(1);
        self.scroll_offset.row = if down {
            self.scroll_offset.row.saturating_add(step).min(max_row)
        } else {
            self.scroll_offset.row.saturating_sub(step)
        };
        self.clamp_cursor_to_view();
    }

    fn clamp_cursor_to_view(&mut self) {
        if self.size.height == 0 {
            return;
        }
        let top = self.scroll_offset.row;
        let bottom = top.saturating_add(self.size.height.saturating_sub(1));
        let last_line = self.buffer.height().saturating_sub(1);
        self.text_location.line_idx = self
            .text_location
            .line_idx
            .clamp(top, bottom)
            .min(last_line);
        self.snap_to_valid_grapheme();
    }
    */

    #[allow(clippy::arithmetic_side_effects)]
    fn move_right(&mut self) {
        let grapheme_count = self.buffer.grapheme_count(self.text_location.line_idx);
        if self.text_location.grapheme_idx < grapheme_count {
            self.text_location.grapheme_idx += 1;
        } else {
            self.move_to_start_of_line();
            self.move_down(1);
        }
    }
    #[allow(clippy::arithmetic_side_effects)]
    fn move_left(&mut self) {
        if self.text_location.grapheme_idx > 0 {
            self.text_location.grapheme_idx -= 1;
        } else if self.text_location.line_idx > 0 {
            self.move_up(1);
            self.move_to_end_of_line();
        }
    }
    fn move_to_start_of_line(&mut self) {
        self.text_location.grapheme_idx = 0;
    }
    fn move_to_end_of_line(&mut self) {
        self.text_location.grapheme_idx = self.buffer.grapheme_count(self.text_location.line_idx);
    }

    fn max_grapheme(&self, line_idx: LineIdx) -> GraphemeIdx {
        let count = self.buffer.grapheme_count(line_idx);
        if self.block_cursor { count.saturating_sub(1) } else { count }
    }

    fn snap_to_valid_grapheme(&mut self) {
        self.text_location.grapheme_idx = min(
            self.text_location.grapheme_idx,
            self.max_grapheme(self.text_location.line_idx),
        );
    }

    fn clamp_block(&mut self) {
        if !self.block_cursor {
            return;
        }
        let last = self.buffer.height().saturating_sub(1);
        self.text_location.line_idx = self.text_location.line_idx.min(last);
        self.snap_to_valid_grapheme();
    }

    fn move_left_in_line(&mut self) {
        self.text_location.grapheme_idx = self.text_location.grapheme_idx.saturating_sub(1);
    }

    fn move_right_in_line(&mut self) {
        let last = self.max_grapheme(self.text_location.line_idx);
        if self.text_location.grapheme_idx < last {
            self.text_location.grapheme_idx = self.text_location.grapheme_idx.saturating_add(1);
        }
    }

    fn snap_to_valid_line(&mut self) {
        self.text_location.line_idx = min(self.text_location.line_idx, self.buffer.height());
    }

    pub fn move_to_screen_position(&mut self, row: RowIdx, col: ColIdx) {
        let vrow = row.saturating_add(self.scroll_offset.row);
        let line_idx = self.buffer.hidden_map().line_of_vrow(vrow);
        let col_target = col
            .saturating_sub(self.gutter_width())
            .saturating_add(self.scroll_offset.col);
        self.text_location = self.buffer.location_for_position(line_idx, col_target);
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }

    /// The fold-button cell for a given screen row/column, if any.
    /// Shared by hover highlighting and click-to-toggle so hit-testing can't drift.
    pub fn fold_cell_line(&self, row: RowIdx, col: ColIdx) -> Option<usize> {
        let gutter = self.gutter_width();
        if gutter > 0 && !self.compact && col == gutter.saturating_sub(2) {
            let hidden = self.buffer.hidden_map();
            let line_idx = hidden.line_of_vrow(row.saturating_add(self.scroll_offset.row));
            if self.buffer.fold_state(line_idx).is_some() {
                return Some(line_idx);
            }
        }
        None
    }

    /// Update the fold-button hover, requesting a redraw only when it changes.
    pub fn set_fold_hover(&mut self, line: Option<usize>) {
        if self.fold_hover != line {
            self.fold_hover = line;
            self.set_needs_redraw(true);
        }
    }

    pub fn handle_mouse_press(&mut self, row: RowIdx, col: ColIdx, clicks: u8) -> bool {
        if let Some(line_idx) = self.fold_cell_line(row, col) {
            self.fold_hover = None;
            self.buffer.set_animate_folds(self.animate());
            self.buffer.fold_toggle(line_idx);
            self.ensure_cursor_visible();
            self.clamp_block();
            self.set_needs_redraw(true);
            return true;
        }
        self.clear_selection();
        self.move_to_screen_position(row, col);
        self.click_unit = match clicks {
            2 => ClickUnit::Word,
            3 => ClickUnit::Line,
            _ => ClickUnit::Char,
        };
        self.click_span = None;
        if self.click_unit != ClickUnit::Char {
            self.selection_inclusive = false;
            let (start, end) = self.unit_span(self.text_location, self.click_unit);
            self.click_span = Some((start, end));
            self.selection_anchor = Some(start);
            self.text_location = end;
            self.scroll_text_location_into_view();
        }
        self.set_needs_redraw(true);
        false
    }

    pub fn set_block_cursor(&mut self, on: bool) {
        if self.block_cursor != on {
            self.block_cursor = on;
            self.set_needs_redraw(true);
        }
        self.normalize_cursor();
    }

    pub fn normalize_cursor(&mut self) {
        self.ensure_cursor_visible();
        let before = (self.text_location.line_idx, self.text_location.grapheme_idx);
        self.clamp_block();
        if before != (self.text_location.line_idx, self.text_location.grapheme_idx) {
            self.scroll_text_location_into_view();
            self.set_needs_redraw(true);
        }
    }

    pub fn leave_insert(&mut self) {
        self.text_location.grapheme_idx = self.text_location.grapheme_idx.saturating_sub(1);
        self.set_needs_redraw(true);
    }

    pub fn step_for_append(&mut self) {
        self.clear_selection();
        let count = self.buffer.grapheme_count(self.text_location.line_idx);
        self.text_location.grapheme_idx = self.text_location.grapheme_idx.saturating_add(1).min(count);
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
    }

    pub fn handle_mouse_drag(&mut self, row: RowIdx, col: ColIdx, step: usize) {
        self.fold_hover = None; // dragging may scroll the viewport
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.text_location);
            self.selection_inclusive = false;
        }
        self.drag_to(row, col, step);
    }

    pub fn handle_mouse_release(&mut self, row: RowIdx, col: ColIdx) -> Option<String> {
        let mut selected = None;
        if self.selection_anchor.is_some() {
            self.drag_to(row, col, 1);
            if self.has_selection() {
                if let Some(text) = self.selection_text() {
                    self.register = Register { content: text.clone(), linewise: false };
                    selected = Some(text);
                }
            } else {
                self.clear_selection();
            }
            self.set_needs_redraw(true);
        }
        selected
    }

    pub fn reload_languages(&mut self) -> (usize, Vec<String>) {
        let report = highlighter::reload_plugins();
        self.buffer.refresh_language();
        self.set_needs_redraw(true);
        report
    }

    pub fn register_text(&self) -> String {
        self.register.content.clone()
    }

    pub fn is_drag_edge(&self, row: RowIdx, col: ColIdx) -> bool {
        let rel = col.saturating_sub(self.gutter_width());
        row == 0
            || row.saturating_add(1) >= self.size.height
            || rel == 0
            || rel.saturating_add(1) >= self.text_width()
    }

    fn drag_to(&mut self, row: RowIdx, col: ColIdx, step: usize) {
        let step = step.max(1);
        let height = self.size.height;
        let text_width = self.text_width();
        let rel = col.saturating_sub(self.gutter_width());
        let scroll = self.scroll_offset;

        let line_idx = if row == 0 && scroll.row > 0 {
            scroll.row.saturating_sub(step)
        } else if row.saturating_add(1) >= height {
            scroll.row.saturating_add(height).saturating_sub(1).saturating_add(step)
        } else {
            row.saturating_add(scroll.row)
        };
        let col_target = if rel == 0 && scroll.col > 0 {
            scroll.col.saturating_sub(step)
        } else if rel.saturating_add(1) >= text_width {
            scroll.col.saturating_add(text_width).saturating_sub(1).saturating_add(step)
        } else {
            rel.saturating_add(scroll.col)
        };

        let line_idx = self.buffer.hidden_map().line_of_vrow(line_idx);
        let target = self.buffer.location_for_position(line_idx, col_target);
        match (self.click_unit, self.click_span) {
            (ClickUnit::Char, _) | (_, None) => self.text_location = target,
            (unit, Some((origin_start, origin_end))) => {
                // Extend by whole words / lines, keeping the originally clicked unit selected.
                let (start, end) = self.unit_span(target, unit);
                let before = (start.line_idx, start.grapheme_idx)
                    < (origin_start.line_idx, origin_start.grapheme_idx);
                let (anchor, cursor) = if before {
                    (origin_end, start)
                } else {
                    (origin_start, end)
                };
                self.selection_anchor = Some(anchor);
                self.text_location = cursor;
            }
        }
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
    }

    fn unit_span(&self, at: Location, unit: ClickUnit) -> (Location, Location) {
        let at = self.buffer.clamp_location(at);
        let line_idx = at.line_idx;
        let count = self.buffer.grapheme_count(line_idx);
        match unit {
            ClickUnit::Char => (at, at),
            ClickUnit::Line => {
                let start = Location { line_idx, grapheme_idx: 0 };
                let end = if line_idx.saturating_add(1) < self.buffer.height() {
                    Location { line_idx: line_idx.saturating_add(1), grapheme_idx: 0 }
                } else {
                    Location { line_idx, grapheme_idx: count }
                };
                (start, end)
            }
            ClickUnit::Word => {
                if count == 0 {
                    return (at, at);
                }
                let class_at = |idx: usize| {
                    self.buffer
                        .grapheme_at(Location { line_idx, grapheme_idx: idx })
                        .map(|g| grapheme_class(&g))
                };
                let idx = at.grapheme_idx.min(count.saturating_sub(1));
                let Some(target) = class_at(idx) else {
                    return (at, at);
                };
                let mut start = idx;
                while start > 0 && class_at(start.saturating_sub(1)) == Some(target) {
                    start = start.saturating_sub(1);
                }
                let mut end = idx.saturating_add(1);
                while end < count && class_at(end) == Some(target) {
                    end = end.saturating_add(1);
                }
                (
                    Location { line_idx, grapheme_idx: start },
                    Location { line_idx, grapheme_idx: end },
                )
            }
        }
    }

    pub fn scroll_view_up(&mut self) {
        const STEP: usize = 3;
        self.scroll_offset.row = self.scroll_offset.row.saturating_sub(STEP);
        self.fold_hover = None; // line numbers under the old cell now differ
        self.set_needs_redraw(true);
    }
    pub fn scroll_view_down(&mut self) {
        const STEP: usize = 3;
        let max_row = self.visible_height().saturating_sub(1);
        self.scroll_offset.row = self.scroll_offset.row.saturating_add(STEP).min(max_row);
        self.fold_hover = None;
        self.set_needs_redraw(true);
    }

    fn has_selection(&self) -> bool {
        self.selection_range().is_some_and(|(start, end)| {
            start.line_idx != end.line_idx || start.grapheme_idx != end.grapheme_idx
        })
    }

    fn scrollbar_thumb_range(total_lines: LineIdx, height: usize, scroll_top: RowIdx) -> (usize, usize) {
        let total_lines = total_lines.max(1);
        if total_lines <= height {
            return (0, height);
        }
        let thumb_len = (height.saturating_mul(height) / total_lines).max(1).min(height);
        let max_start = height.saturating_sub(thumb_len);
        let thumb_start = (scroll_top.saturating_mul(height) / total_lines).min(max_start);
        (thumb_start, thumb_start.saturating_add(thumb_len))
    }

    pub fn toggle_autopairs(&mut self) -> bool {
        self.no_autopairs = !self.no_autopairs;
        !self.no_autopairs
    }

    fn handle_autopair(&mut self, c: char) -> bool {
        if self.no_autopairs {
            return false;
        }
        let at = self.text_location;
        let before = self.buffer.char_before(at);
        let after = self.buffer.char_after(at);

        if matches!(c, ')' | ']' | '}' | '"' | '\'' | '`') && after == Some(c) {
            self.handle_move_command(Move::Right);
            return true;
        }

        let closer = match c {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            '"' | '\'' | '`' => c,
            _ => return false,
        };
        let is_quote = closer == c;

        if after.is_some_and(|a| {
            a.is_alphanumeric() || matches!(a, '%' | '\'' | '[' | '"' | '.' | '`' | '$')
        }) {
            return false;
        }
        if is_quote {
            if before.is_some_and(|b| b.is_alphanumeric() || b == '\\') {
                return false;
            }
            if c == '\'' && self.buffer.supports_lifetime() {
                return false;
            }
        } else if self.buffer.rest_is_unbalanced(at, c, closer) {
            return false;
        }
        if self.buffer.is_in_string(at) {
            return false;
        }

        self.insert_text_at(at, &format!("{c}{closer}"));
        self.text_location = Location {
            line_idx: at.line_idx,
            grapheme_idx: at.grapheme_idx.saturating_add(1),
        };
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
        true
    }


    fn gutter_width(&self) -> usize {
        if self.no_line_numbers {
            return 0;
        }
        let digits = self.buffer.height().max(1).to_string().len();
        let gutter = if self.compact {
            digits.max(2).saturating_add(1)
        } else {
            digits.max(3).saturating_add(3)
        };
        if self.size.width > gutter.saturating_add(2) { gutter } else { 0 }
    }

    fn text_width(&self) -> usize {
        self.size
            .width
            .saturating_sub(self.gutter_width())
            .saturating_sub(usize::from(!self.compact))   // scrollbar column
            .max(1)
    }

    pub fn toggle_line_numbers(&mut self) -> bool {
        self.no_line_numbers = !self.no_line_numbers;
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        !self.no_line_numbers
    }

    pub fn seal_undo_group(&mut self) {
        self.buffer.seal_undo_group(self.text_location);
    }

    pub fn try_undo(&mut self) -> bool {
        self.clear_selection();
        match self.buffer.undo(self.text_location) {
            Some(location) => {
                self.restore_cursor(location);
                true
            }
            None => false,
        }
    }

    pub fn try_redo(&mut self) -> bool {
        self.clear_selection();
        match self.buffer.redo(self.text_location) {
            Some(location) => {
                self.restore_cursor(location);
                true
            }
            None => false,
        }
    }

    fn restore_cursor(&mut self, location: Location) {
        self.text_location = location;
        self.snap_to_valid_line();
        self.snap_to_valid_grapheme();
        self.clamp_block();
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }

    pub fn file_path(&self) -> Option<String> {
        self.buffer
            .get_file_info()
            .get_path()
            .and_then(|path| path.to_str())
            .map(str::to_string)
    }

    pub fn set_big_edit(&mut self, lines: usize) {
        self.big_edit_min = lines;
    }
    pub fn take_big_edit(&mut self) -> Option<BigEdit> {
        self.big_edit.take()
    }
    fn note_big(&mut self, edit: BigEdit) {
        let (BigEdit::Deleted(n) | BigEdit::Pasted(n)) = edit;
        if self.big_edit_min > 0 && n >= self.big_edit_min {
            self.big_edit = Some(edit);
        }
    }
    pub fn disk_changed(&self) -> bool {
        self.buffer.disk_changed()
    }
    pub fn start_line_fill(&mut self) {
        if self.animate() {
            self.fill_anim = Some(Instant::now());
            self.set_needs_redraw(true);
        }
    }

    pub fn clear(&mut self) {
        let size = self.size;
        let (no_autopairs, no_line_numbers) = (self.no_autopairs, self.no_line_numbers);
        let (scope_guide, scope_highlight, scope_brackets) =
            (self.scope_guide, self.scope_highlight, self.scope_brackets);
        let yanks = std::mem::take(&mut self.yanks);
        let (animations, minimal_gutter) = (self.animations, self.minimal_gutter);
        let compact = self.compact;
        let popup_animation = self.popup_animation;
        let big_edit_min = self.big_edit_min;
        *self = Self::default();
        self.big_edit_min = big_edit_min;
        self.popup_animation = popup_animation;
        self.yanks = yanks;
        self.size = size;
        self.animations = animations;
        self.compact = compact;
        self.minimal_gutter = minimal_gutter;
        self.no_autopairs = no_autopairs;
        self.no_line_numbers = no_line_numbers;
        self.scope_guide = scope_guide;
        self.scope_highlight = scope_highlight;
        self.scope_brackets = scope_brackets;
        self.set_needs_redraw(true);
    }

    pub fn last_search(&self) -> Option<&str> {
        self.last_search.as_deref()
    }

    pub fn set_search_pattern(&mut self, pattern: &str) {
        self.last_search = Some(pattern.to_string());
        self.hl_search = true;
        self.set_needs_redraw(true);
    }

    pub fn set_compact(&mut self, on: bool) {
        if self.compact != on {
            self.compact = on;
            self.scroll_text_location_into_view();
            self.set_needs_redraw(true);
        }
    }

    fn animate(&self) -> bool {
        self.animations && !self.compact
    }

    pub fn clear_search_highlight(&mut self) {
        self.hl_search = false;
        self.set_needs_redraw(true);
    }

    pub fn set_style(&mut self, minimal_gutter: bool, animations: bool) {
        self.minimal_gutter = minimal_gutter;
        self.animations = animations;
        self.shown_row = self.scroll_offset.row;
        self.set_needs_redraw(true);
    }

    fn chase(from: usize, to: usize) -> usize {
        let diff = from.abs_diff(to);
        let step = (diff.saturating_mul(2) / 5).max(1).min(diff);
        if to > from { from + step } else { from - step }
    }

    fn sync_shown(&mut self) {
        let animate = self.animate();
        let far_r = self.size.height.saturating_mul(3).max(6);
        let far_c = self.size.width.saturating_mul(2).max(12);
        let snap = |shown: usize, to: usize, far: usize| {
            !animate || shown.abs_diff(to) < 2 || shown.abs_diff(to) >= far
        };
        if snap(self.shown_row, self.scroll_offset.row, far_r) {
            self.shown_row = self.scroll_offset.row;
        }
        if snap(self.shown_col, self.scroll_offset.col, far_c) {
            self.shown_col = self.scroll_offset.col;
        }
    }

    pub fn animating(&self) -> bool {
        self.animate()
            && (self.shown_row != self.scroll_offset.row
                || self.shown_col != self.scroll_offset.col
                || self.guide_anim.is_some() || self.buffer.fold_anim_pending() || self.comp_anim.is_some() || self.cursor_gliding() || self.fill_anim.is_some())
    }

    pub fn tick_animation(&mut self) -> bool {
        if !self.animate() {
            return false;
        }
        let mut changed = false;
        if self.shown_row != self.scroll_offset.row {
            self.shown_row = Self::chase(self.shown_row, self.scroll_offset.row);
            changed = true;
        }
        if self.shown_col != self.scroll_offset.col {
            self.shown_col = Self::chase(self.shown_col, self.scroll_offset.col);
            changed = true;
        }
        if let Some(start) = self.guide_anim {
            if start.elapsed().as_millis() > GUIDE_ANIM_MS {
                self.guide_anim = None;
            }
            changed = true;
        }
        if self.tick_cursor() {
            changed = true;
        }
        if let Some(start) = self.comp_anim {
            if start.elapsed().as_millis() >= POPUP_ANIM_MS {
                self.comp_anim = None;
            }
            changed = true;
        }
        if self.buffer.fold_anim_pending() {
            if !self.buffer.fold_animating() {
                self.buffer.finish_fold_anim();
                self.ensure_cursor_visible();
            }
            changed = true;
        }
        if let Some(s) = self.fill_anim {
            if s.elapsed().as_secs_f32() * 1000.0 >= FILL_MS {
                self.fill_anim = None;
            }
            changed = true;
        }
        if changed {
            self.set_needs_redraw(true);
        }
        changed
    }

    fn guide_reveal(&mut self, active: &[(usize, LineIdx, LineIdx)]) -> Option<(LineIdx, usize)> {
        if !self.animate() { return None; }
        let key = active.first().map(|&(col, open, _)| (col, open));
        if key != self.guide_key {
            self.guide_key = key;
            self.guide_anim = key.map(|_| Instant::now());
        }
        let start = self.guide_anim?;
        let &(_, open, close) = active.first()?;
        let span = u128::try_from(close.saturating_sub(open).max(24)).unwrap_or(24);
        let radius = start.elapsed().as_millis().saturating_mul(span) / GUIDE_ANIM_MS;
        Some((self.text_location.line_idx, usize::try_from(radius).unwrap_or(usize::MAX)))
    }

    pub fn cursor_line(&self) -> LineIdx {
        self.text_location.line_idx
    }

    pub fn line_count(&self) -> LineIdx {
        self.buffer.height()
    }

    pub fn selection_lines(&self) -> Option<(LineIdx, LineIdx)> {
        self.selection_range().map(|(start, end)| (start.line_idx, end.line_idx))
    }

    pub fn goto_line(&mut self, line_idx: LineIdx) {
        let from = self.text_location;
        self.seal_undo_group();
        self.clear_selection();
        self.text_location = Location {
            line_idx: line_idx.min(self.buffer.height().saturating_sub(1)),
            grapheme_idx: 0,
        };
        self.note_jump(from);
        self.reveal_location();
        self.set_needs_redraw(true);
    }

    pub fn goto_position(&mut self, line_idx: Option<LineIdx>, grapheme_idx: Option<GraphemeIdx>) {
        let from = self.text_location;
        self.seal_undo_group();
        self.clear_selection();
        let line_idx = line_idx
            .unwrap_or(self.text_location.line_idx)
            .min(self.buffer.height().saturating_sub(1));
        let grapheme_idx = grapheme_idx
            .unwrap_or(0)
            .min(self.max_grapheme(line_idx));
        self.text_location = Location { line_idx, grapheme_idx };
        self.note_jump(from);
        self.reveal_location();
        self.set_needs_redraw(true);
    }

    fn reveal_location(&mut self) {
        self.buffer.fold_reveal(self.text_location.line_idx);
        let (row, col) = (self.scroll_offset.row, self.scroll_offset.col);
        self.scroll_text_location_into_view();
        if self.scroll_offset.row != row || self.scroll_offset.col != col {
            self.center_text_location();
        }
    }

    fn after(&self, at: Location) -> Location {
        Location {
            line_idx: at.line_idx,
            grapheme_idx: at
                .grapheme_idx
                .saturating_add(1)
                .min(self.buffer.grapheme_count(at.line_idx)),
        }
    }

    pub fn vim_search_update(&mut self, query: &str, forward: bool) {
        let Some(info) = self.search_info.as_mut() else {
            return;
        };
        info.query = Some(Line::from(query));
        let (prev_location, prev_scroll) = (info.prev_location, info.prev_scroll_offset);
        let found = if query.is_empty() {
            None
        } else if forward {
            self.buffer.search_forward(query, self.after(prev_location))
        } else {
            self.buffer.search_backward(query, prev_location)
        };
        match found {
            Some(location) => {
                self.text_location = location;
                self.reveal_location();
            }
            None => {
                self.text_location = prev_location;
                self.scroll_offset = prev_scroll;
            }
        }
        self.set_needs_redraw(true);
    }

    pub fn vim_search_accept(&mut self, typed: &str, forward: bool) -> Result<(), String> {
        let (prev_location, prev_scroll) = match &self.search_info {
            Some(info) => (info.prev_location, info.prev_scroll_offset),
            None => (self.text_location, self.scroll_offset),
        };
        let pattern = if typed.is_empty() {
            self.last_search.clone()
        } else {
            Some(typed.to_string())
        };
        let Some(pattern) = pattern else {
            self.dismiss_search();
            return Err("E35: No previous regular expression".to_string());
        };
        self.last_search = Some(pattern.clone());
        self.search_backward = !forward;
        self.hl_search = true;
        let found = if forward {
            self.buffer.search_forward(&pattern, self.after(prev_location))
        } else {
            self.buffer.search_backward(&pattern, prev_location)
        };
        self.exit_search();
        match found {
            Some(location) => {
                self.text_location = location;
                self.note_jump(prev_location);
                self.scroll_offset = prev_scroll;
                self.reveal_location();
                self.set_needs_redraw(true);
                Ok(())
            }
            None => {
                self.text_location = prev_location;
                self.scroll_offset = prev_scroll;
                self.set_needs_redraw(true);
                Err(format!("E486: Pattern not found: {pattern}"))
            }
        }
    }

    pub fn search_repeat(&mut self, reverse: bool) -> Result<(), String> {
        let Some(pattern) = self.last_search.clone() else {
            return Err("E35: No previous regular expression".to_string());
        };
        let backward = self.search_backward != reverse;
        let at = self.text_location;
        let found = if backward {
            self.buffer.search_backward(&pattern, at)
        } else {
            self.buffer.search_forward(&pattern, self.after(at))
        };
        self.hl_search = true;
        self.set_needs_redraw(true);
        match found {
            Some(location) => {
                self.clear_selection();
                self.text_location = location;
                self.note_jump(at);
                self.reveal_location();
                Ok(())
            }
            None => Err(format!("E486: Pattern not found: {pattern}")),
        }
    }

    pub fn search_word_under_cursor(&mut self, forward: bool) -> Result<(), String> {
        let (word, start) = self
            .word_under_cursor()
            .ok_or_else(|| "E348: No string under cursor".to_string())?;
        self.text_location.grapheme_idx = start;
        self.last_search = Some(word);
        self.search_backward = !forward;
        self.search_repeat(false)
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn word_under_cursor(&self) -> Option<(String, GraphemeIdx)> {
        let line = self.buffer.line_content(self.text_location.line_idx)?;
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let is_word = |g: &str| g.chars().all(|c| c.is_alphanumeric() || c == '_');
        let mut start = self.text_location.grapheme_idx;
        while start < graphemes.len() && !is_word(graphemes[start]) {
            start += 1;
        }
        if start >= graphemes.len() {
            return None;
        }
        while start > 0 && is_word(graphemes[start - 1]) {
            start -= 1;
        }
        let mut end = start;
        while end < graphemes.len() && is_word(graphemes[end]) {
            end += 1;
        }
        Some((graphemes[start..end].concat(), start))
    }

    fn location_before(&self, at: Location) -> Option<Location> {
        if at.grapheme_idx > 0 {
            Some(Location { line_idx: at.line_idx, grapheme_idx: at.grapheme_idx.saturating_sub(1) })
        } else if at.line_idx > 0 {
            let line_idx = at.line_idx.saturating_sub(1);
            Some(Location { line_idx, grapheme_idx: self.buffer.grapheme_count(line_idx) })
        } else {
            None
        }
    }

    fn wrapping_pair(&self, inner: (Location, Location)) -> Option<(Location, Location)> {
        let key = |at: Location| (at.line_idx, at.grapheme_idx);
        let mut probe = self.location_before(inner.0)?;
        for _ in 0..16 {
            let candidate = self.buffer.enclosing_pair(probe)?;
            if key(candidate.0) < key(inner.0) && key(candidate.1) > key(inner.1) {
                return Some(candidate);
            }
            probe = self.location_before(candidate.0)?;
        }
        None
    }

    fn enclosing_scopes(&self) -> Vec<(Location, Location)> {
        let key = |at: Location| (at.line_idx, at.grapheme_idx);
        let cursor = self.text_location;
        let mut out = Vec::new();
        let Some(mut pair) = self.buffer.enclosing_pair(cursor) else {
            return out;
        };
        for _ in 0..16 {
            let past_open = key(pair.0) < key(cursor)
                || (self.block_cursor && key(pair.0) == key(cursor));
            let before_close = key(cursor) <= key(pair.1);
            let right_of_guide =
                cursor.grapheme_idx >= self.buffer.indent_end(pair.0.line_idx);
            if past_open
                && before_close
                && right_of_guide
                && pair.0.line_idx != pair.1.line_idx
            {
                out.push(pair);
            }
            match self.wrapping_pair(pair) {
                Some(outer) => pair = outer,
                None => break,
            }
        }
        out
    }

    fn draw_indent_guides(
        &self,
        guides: &Guides,
        row: RowIdx,
        line_idx: LineIdx,
        gutter: usize,
        bg: Color,
        selected: Option<(GraphemeIdx, GraphemeIdx)>,
    ) -> Result<(), Error> {
        let lead = self.buffer.guide_indent(line_idx);
        let left = self.shown_col;
        let right = left.saturating_add(self.text_width());
        let unit = guides.unit.max(1);
        for col in left..lead.min(right) {
            let lit = guides.active.iter().position(|&(active_col, open, close)| {
                col == active_col && line_idx > open && line_idx < close
                    && guides.reveal.map_or(true, |(c, r)| line_idx.abs_diff(c) <= r)
            });
            if lit.is_none() && col % unit != 0 {
                continue;
            }
            if selected.is_some_and(|(start, end)| col >= start && col < end) {
                continue;
            }
            let color = match lit {
                Some(0) => INDENT_GUIDE_ACTIVE,
                Some(_) => INDENT_GUIDE_OUTER,
                None => INDENT_GUIDE,
            };
            Terminal::print_styled_at(
                Position { row, col: gutter.saturating_add(col.saturating_sub(left)) },
                "│",
                color,
                bg,
                false,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::arithmetic_side_effects)]
    pub fn substitute(&mut self, first: LineIdx, last: LineIdx, sub: &Substitute) -> (usize, usize) {
        self.seal_undo_group();
        self.buffer.mark_cursor(self.text_location);
        let (mut total, mut lines) = (0usize, 0usize);
        let mut last_changed = None;
        let end = last.min(self.buffer.height().saturating_sub(1));
        for idx in first..=end {
            let Some(content) = self.buffer.line_content(idx) else {
                continue;
            };
            let Some((replaced, count)) = replace_in_line(&content, sub) else {
                continue;
            };
            if !sub.count_only {
                self.buffer.replace_line_text(idx, &replaced);
            }
            total += count;
            lines += 1;
            last_changed = Some(idx);
        }
        if !sub.count_only {
            if let Some(idx) = last_changed {
                self.text_location = Location { line_idx: idx, grapheme_idx: 0 };
            }
        }
        self.clear_selection();
        self.seal_undo_group();
        self.snap_to_valid_line();
        self.snap_to_valid_grapheme();
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        (total, lines)
    }

    pub fn cursor_location(&self) -> Location {
        self.text_location
    }

    pub fn qr_next(&mut self, pattern: &str, from: Location) -> Option<Location> {
        let start = self.buffer.search_forward_nowrap(pattern, from)?;
        self.buffer.fold_reveal(start.line_idx);
        let len = Line::from(pattern).grapheme_count();
        self.selection_anchor = Some(start);
        self.selection_inclusive = false;
        self.text_location = Location {
            line_idx: start.line_idx,
            grapheme_idx: start.grapheme_idx.saturating_add(len),
        };
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        Some(start)
    }

    pub fn qr_replace(&mut self, pattern: &str, with: &str, start: Location) -> Location {
        if self.guard_read_only() {
            return start; 
        }
        self.clear_selection();
        self.buffer.mark_cursor(start);
        let inserted = self.buffer.replace_at(start, pattern, with);
        let after = Location {
            line_idx: start.line_idx,
            grapheme_idx: start.grapheme_idx.saturating_add(inserted),
        };
        self.text_location = after;
        self.set_needs_redraw(true);
        after
    }

    pub fn qr_skip(&mut self, pattern: &str, start: Location) -> Location {
        self.clear_selection();
        self.set_needs_redraw(true);
        Location {
            line_idx: start.line_idx,
            grapheme_idx: start
                .grapheme_idx
                .saturating_add(Line::from(pattern).grapheme_count()),
        }
    }

    pub fn qr_finish(&mut self) {
        self.clear_selection();
        self.seal_undo_group();
        self.set_needs_redraw(true);
    }

    pub fn set_scope(&mut self, guide: bool, highlight: bool, brackets: bool) {
        if self.scope_guide != guide
            || self.scope_highlight != highlight
            || self.scope_brackets != brackets
        {
            self.scope_guide = guide;
            self.scope_highlight = highlight;
            self.scope_brackets = brackets;
            self.scroll_text_location_into_view();
            self.set_needs_redraw(true);
        }
    }

    pub fn configure_yanks(&mut self, enabled: bool, max_entries: usize, max_entry_bytes: usize) {
        self.yanks.configure(enabled, max_entries, max_entry_bytes);
    }

    pub const fn yank_history_enabled(&self) -> bool {
        self.yanks.is_enabled()
    }

    pub fn yank_count(&self) -> usize {
        self.yanks.len()
    }

    pub fn yank_entries(&self) -> Vec<String> {
        self.yanks.entries().map(str::to_string).collect()
    }

    pub fn yank_entry(&self, idx: usize) -> Option<String> {
        self.yanks.get(idx).map(str::to_string)
    }

    pub fn clear_yanks(&mut self) -> usize {
        self.yanks.clear()
    }

    pub fn tab_info(&self) -> (String, bool) {
        (self.buffer.get_file_info().to_string(), self.buffer.is_dirty())
    }

    pub fn inherit_shared(&mut self, from: &mut Self) {
        self.yanks = std::mem::take(&mut from.yanks);
        self.register = from.register.clone();
        self.last_search = from.last_search.clone();
        self.search_backward = from.search_backward;
        self.hl_search = from.hl_search;
        self.no_autopairs = from.no_autopairs;
        self.no_line_numbers = from.no_line_numbers;
        self.size = from.size;
        self.clear_selection();
        self.search_info = None;
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
    }


    fn guard_read_only(&mut self) -> bool {
        if self.buffer.is_read_only() {
            self.blocked_edit = true;
            true
        } else {
            false
        }
    }

    pub fn take_blocked(&mut self) -> bool {
        std::mem::take(&mut self.blocked_edit)
    }

    pub const fn is_read_only(&self) -> bool {
        self.buffer.is_read_only()
    }

    pub fn toggle_read_only(&mut self) -> bool {
        let on = !self.buffer.is_read_only();
        self.buffer.set_read_only(on);
        self.set_needs_redraw(true);
        on
    }

    pub fn encoding_label(&self) -> &'static str {
        self.buffer.encoding_label()
    }

    pub fn set_encoding(&mut self, name: &str) -> Result<&'static str, String> {
        let encoding = Encoding::parse(name).ok_or_else(|| unknown_encoding(name))?;
        self.buffer.set_encoding(encoding);
        self.set_needs_redraw(true);
        Ok(encoding.label())
    }

    pub fn set_line_ending(&mut self, name: &str) -> Result<&'static str, String> {
        let label = self
            .buffer
            .set_line_ending_named(name)
            .ok_or_else(|| format!("E474: Unknown line ending: {name} (lf, crlf, cr)"))?;
        self.set_needs_redraw(true);
        Ok(label)
    }

    pub fn reload_with_encoding(&mut self, file_name: &str, name: &str) -> Result<&'static str, String> {
        let encoding = Encoding::parse(name).ok_or_else(|| unknown_encoding(name))?;
        let buffer = Buffer::load_with(file_name, Some(encoding))
            .map_err(|err| crate::editor::errors::reason(&err))?;
        let label = buffer.encoding_label();
        self.buffer = buffer;
        self.clear_selection();
        self.search_info = None;
        let last = self.buffer.height().saturating_sub(1);
        self.text_location.line_idx = min(self.text_location.line_idx, last);
        self.snap_to_valid_grapheme();
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        Ok(label)
    }

    // ---- jump list ----------------------------------------------------------

    fn record_jump(&mut self, from: Location) {
        self.jumps.retain(|jump| jump.line_idx != from.line_idx);
        self.jumps.push(from);
        if self.jumps.len() > MAX_JUMPS {
            self.jumps.remove(0);
        }
        self.jump_pos = self.jumps.len();
    }

    fn note_jump(&mut self, from: Location) {
        if from.line_idx != self.text_location.line_idx {
            self.record_jump(from);
        }
    }

    fn go_to_jump(&mut self, at: Location) {
        let line_idx = at.line_idx.min(self.buffer.height().saturating_sub(1));
        let grapheme_idx = at.grapheme_idx.min(self.max_grapheme(line_idx));
        self.text_location = Location { line_idx, grapheme_idx };
        self.reveal_location();
    }

    fn jump_back(&mut self) {
        let here = self.text_location;
        if self.jump_pos >= self.jumps.len() {
            match self.jumps.last_mut() {
                Some(last) if last.line_idx == here.line_idx => *last = here,
                _ => self.jumps.push(here),
            }
            self.jump_pos = self.jumps.len().saturating_sub(1);
        }
        let Some(target) = self.jump_pos.checked_sub(1) else {
            return;
        };
        self.jump_pos = target;
        if let Some(at) = self.jumps.get(target).copied() {
            self.go_to_jump(at);
        }
    }

    fn jump_forward(&mut self) {
        let next = self.jump_pos.saturating_add(1);
        if next >= self.jumps.len() {
            return;
        }
        self.jump_pos = next;
        if let Some(at) = self.jumps.get(next).copied() {
            self.go_to_jump(at);
        }
    }

    pub fn jump_lines(&self) -> Vec<usize> {
        self.jumps.iter().map(|jump| jump.line_idx).collect()
    }

    pub fn accept_search(&mut self) {
        let from = self.search_info.as_ref().map(|info| info.prev_location);
        if let Some(from) = from {
            self.note_jump(from);
        }
        self.exit_search();
    }

    pub fn take_want_insert(&mut self) -> bool {
        std::mem::take(&mut self.want_insert)
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    fn text_between(&self, start: Location, end: Location) -> String {
        let mut out = String::new();
        for line_idx in start.line_idx..=end.line_idx {
            let count = self.buffer.grapheme_count(line_idx);
            let from = if line_idx == start.line_idx { start.grapheme_idx } else { 0 };
            let to = if line_idx == end.line_idx { end.grapheme_idx } else { count };
            if let Some(s) = self.buffer.substring(line_idx, from..to) {
                out.push_str(&s);
            }
            if line_idx != end.line_idx {
                out.push('\n');
            }
        }
        out
    }

    fn lines_text(&self, first: LineIdx, last: LineIdx) -> String {
        (first..=last)
            .filter_map(|i| self.buffer.line_content(i))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// (start, exclusive end, linewise). For linewise, both lines are inclusive.
    fn operator_range(&mut self, op: Op, target: Target, count: usize) -> Option<(Location, Location, bool)> {
        let here = self.text_location;
        let last_line = self.buffer.height().checked_sub(1)?;
        let col0 = |line_idx| Location { line_idx, grapheme_idx: 0 };
        match target {
            Target::Line => {
                let mut end = self.buffer.closed_fold_end(here.line_idx).unwrap_or(here.line_idx);
                for _ in 1..count {
                    let next = end.saturating_add(1);
                    if next > last_line {
                        break;
                    }
                    end = self.buffer.closed_fold_end(next).unwrap_or(next);
                }
                Some((col0(here.line_idx), col0(end.min(last_line)), true))
            }
            Target::Object { obj, around } => self
                .buffer
                .text_object(obj, around, here)
                .map(|r| (r.start, r.end, r.linewise)),
            Target::Motion(motion) => {
                let mut m = motion;
                if op == Op::Change {
                    let on_word = self
                        .buffer
                        .grapheme_at(here)
                        .is_some_and(|g| !g.trim().is_empty());
                    if on_word {
                        m = match m {
                            Move::WordForward => Move::WordEnd,
                            Move::BigWordForward => Move::BigWordEnd,
                            other => other,
                        };
                    }
                }
                if matches!(m, Move::Left) && here.grapheme_idx == 0 {
                    return None;
                }
                let (saved_block, saved_scroll) = (self.block_cursor, self.scroll_offset);
                self.block_cursor = false;
                let mut moved = false;
                for _ in 0..count {
                    let before = self.text_location;
                    self.move_cursor(m);
                    if (before.line_idx, before.grapheme_idx)
                        == (self.text_location.line_idx, self.text_location.grapheme_idx)
                    {
                        break;
                    }
                    moved = true;
                }
                let target_loc = self.text_location;
                self.text_location = here;
                self.block_cursor = saved_block;
                self.scroll_offset = saved_scroll;
                if !moved {
                    return None;
                }
                let linewise = matches!(
                    m,
                    Move::Up | Move::Down | Move::BufferStart | Move::BufferEnd
                        | Move::ScreenTop | Move::ScreenMiddle | Move::ScreenBottom | Move::NextLineStart | Move::PrevLineStart
                );
                if linewise {
                    let (a, b) = (here.line_idx.min(target_loc.line_idx), here.line_idx.max(target_loc.line_idx));
                    let b = self.buffer.closed_fold_end(b).unwrap_or(b);
                    return Some((col0(a), col0(b.min(last_line)), true));
                }
                let incl = m.is_inclusive() && !matches!(m, Move::Find { forward: false, .. });
                let plus = |at: Location, on: bool| Location {
                    line_idx: at.line_idx,
                    grapheme_idx: at
                        .grapheme_idx
                        .saturating_add(usize::from(on))
                        .min(self.buffer.grapheme_count(at.line_idx)),
                };
                let key = |l: Location| (l.line_idx, l.grapheme_idx);
                let (start, mut end) = if key(target_loc) > key(here) {
                    (here, plus(target_loc, incl))
                } else {
                    (target_loc, plus(here, incl && matches!(m, Move::MatchingBracket)))
                };
                if matches!(m, Move::WordForward | Move::BigWordForward) && end.line_idx > start.line_idx {
                    let line_idx = end.line_idx.saturating_sub(1).max(start.line_idx);
                    end = Location { line_idx, grapheme_idx: self.buffer.grapheme_count(line_idx) };
                }
                Some((start, end, false))
            }
        }
    }

    fn apply_operator(&mut self, op: Op, target: Target, count: usize) {
        let here = self.text_location;
        let Some((start, end, linewise)) = self.operator_range(op, target, count.max(1)) else {
            self.text_location = here;
            return;
        };
        let first = start.line_idx;
        let last = if linewise || end.grapheme_idx > 0 || end.line_idx == start.line_idx {
            end.line_idx
        } else {
            end.line_idx.saturating_sub(1)
        };
        let lines = last.saturating_sub(first).saturating_add(1);
        match op {
            Op::Yank => {
                let text = if linewise {
                    self.lines_text(first, last)
                } else {
                    self.text_between(start, end)
                };
                self.yanks.record(&text);
                self.register = Register { content: text, linewise };
                self.text_location = if linewise {
                    Location {
                        line_idx: first,
                        grapheme_idx: here.grapheme_idx.min(self.max_grapheme(first)),
                    }
                } else {
                    start
                };
                if lines > 2 {
                    self.notice = Some(format!("{lines} lines yanked"));
                }
            }
            Op::Delete | Op::Change => {
                self.note_big(BigEdit::Deleted(lines));
                let text = if linewise {
                    self.lines_text(first, last)
                } else {
                    self.text_between(start, end)
                };
                self.register = Register { content: text, linewise };
                if linewise {
                    let whitespace: String = self
                        .buffer
                        .line_content(first)
                        .unwrap_or_default()
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    if op == Op::Change {
                        self.buffer.replace_line_text(first, &whitespace);
                        for _ in first.saturating_add(1)..=last {
                            self.buffer.delete_line(first.saturating_add(1));
                        }
                        self.text_location = Location {
                            line_idx: first,
                            grapheme_idx: whitespace.chars().count(),
                        };
                    } else {
                        for _ in first..=last {
                            self.buffer.delete_line(first);
                        }
                        let line_idx = first.min(self.buffer.height().saturating_sub(1));
                        self.text_location = Location {
                            line_idx,
                            grapheme_idx: self.buffer.first_non_blank(line_idx),
                        };
                    }
                } else {
                    self.buffer.delete_range(start, end);
                    self.text_location = start;
                }
                if op == Op::Change {
                    self.want_insert = true;
                }
                if lines > 2 {
                    self.notice = Some(format!("{lines} fewer lines"));
                }
            }
            Op::Indent | Op::Dedent => {
                for line_idx in first..=last {
                    if op == Op::Indent {
                        if self.buffer.line_content(line_idx).is_some_and(|l| !l.trim().is_empty()) {
                            for _ in INDENT.chars() {
                                self.buffer.insert_char(' ', Location { line_idx, grapheme_idx: 0 });
                            }
                        }
                    } else {
                        self.buffer.delete_leading_tab_or_spaces(line_idx);
                    }
                }
                self.text_location = Location {
                    line_idx: first,
                    grapheme_idx: self.buffer.first_non_blank(first),
                };
                if lines > 2 {
                    self.notice = Some(format!("{lines} lines shifted"));
                }
            }
            Op::Comment => {
                self.toggle_comment(first, last);
                self.text_location = Location {
                    line_idx: first,
                    grapheme_idx: self.buffer.first_non_blank(first),
                };
            }
        }
        self.clear_selection();
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }

    fn toggle_comment(&mut self, first: LineIdx, last: LineIdx) {
        let Some(prefix) = self.buffer.line_comment() else {
            self.notice = Some("No comment syntax for this file type".to_string());
            return;
        };
        let lines: Vec<(LineIdx, String)> = (first..=last)
            .filter_map(|i| self.buffer.line_content(i).map(|l| (i, l)))
            .filter(|(_, l)| !l.trim().is_empty())
            .collect();
        if lines.is_empty() {
            return;
        }
        let all_commented = lines.iter().all(|(_, l)| l.trim_start().starts_with(prefix.as_str()));
        let indent_of = |l: &str| l.len() - l.trim_start().len();
        let min_indent = lines.iter().map(|(_, l)| indent_of(l)).min().unwrap_or(0);
        for (idx, line) in lines {
            let new = if all_commented {
                let at = indent_of(&line);
                let rest = &line[at + prefix.len()..];
                format!("{}{}", &line[..at], rest.strip_prefix(' ').unwrap_or(rest))
            } else {
                format!("{}{} {}", &line[..min_indent], prefix, &line[min_indent..])
            };
            self.buffer.replace_line_text(idx, &new);
        }
    }

    fn join_lines(&mut self, count: usize) {
        for _ in 0..count.max(2).saturating_sub(1) {
            let l = self.text_location.line_idx;
            if l.saturating_add(1) >= self.buffer.height() {
                break;
            }
            let (Some(cur), Some(next)) = (self.buffer.line_content(l), self.buffer.line_content(l.saturating_add(1)))
            else {
                break;
            };
            let trimmed = next.trim_start();
            let sep = if trimmed.is_empty()
                || cur.is_empty()
                || cur.ends_with(char::is_whitespace)
                || trimmed.starts_with(')')
            {
                ""
            } else {
                " "
            };
            let col = self.buffer.grapheme_count(l);
            self.buffer.replace_line_text(l, &format!("{cur}{sep}{trimmed}"));
            self.buffer.delete_line(l.saturating_add(1));
            self.text_location = Location { line_idx: l, grapheme_idx: col };
        }
        self.set_needs_redraw(true);
    }

    fn toggle_case(&mut self, count: usize) {
        let at = self.text_location;
        let left = self.buffer.grapheme_count(at.line_idx).saturating_sub(at.grapheme_idx);
        let n = count.max(1).min(left);
        let Some(pattern) = self.buffer.substring(at.line_idx, at.grapheme_idx..at.grapheme_idx.saturating_add(n)) else {
            return;
        };
        if n == 0 {
            return;
        }
        let swapped: String = pattern
            .chars()
            .map(|c| {
                if c.is_lowercase() {
                    c.to_uppercase().collect::<String>()
                } else if c.is_uppercase() {
                    c.to_lowercase().collect::<String>()
                } else {
                    c.to_string()
                }
            })
            .collect();
        self.buffer.replace_at(at, &pattern, &swapped);
        self.text_location.grapheme_idx = at.grapheme_idx.saturating_add(n);
        self.clamp_block();
        self.set_needs_redraw(true);
    }

    fn replace_char(&mut self, ch: char, count: usize) {
        let at = self.text_location;
        let n = count.max(1);
        let end = at.grapheme_idx.saturating_add(n);
        if end > self.buffer.grapheme_count(at.line_idx) {
            return;
        }
        let Some(pattern) = self.buffer.substring(at.line_idx, at.grapheme_idx..end) else {
            return;
        };
        self.buffer.replace_at(at, &pattern, &ch.to_string().repeat(n));
        self.text_location.grapheme_idx = end.saturating_sub(1);
        self.set_needs_redraw(true);
    }

    /// Sort lines `first..=last`; returns how many lines remain.
    pub fn sort_lines(&mut self, first: LineIdx, last: LineIdx, args: SortArgs) -> usize {
        if self.guard_read_only() {
            return 0;
        }
        self.seal_undo_group();
        self.buffer.mark_cursor(self.text_location);
        let mut lines: Vec<String> = (first..=last).filter_map(|i| self.buffer.line_content(i)).collect();
        let number = |s: &str| -> Option<i64> {
            let start = s.find(|c: char| c.is_ascii_digit())?;
            let neg = start > 0 && s.as_bytes().get(start - 1) == Some(&b'-');
            let digits: String = s[start..].chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<i64>().ok().map(|n| if neg { -n } else { n })
        };
        let key = |s: &str| if args.ignore_case { s.to_lowercase() } else { s.to_string() };
        if args.numeric {
            lines.sort_by(|a, b| number(a).cmp(&number(b)).then_with(|| key(a).cmp(&key(b))));
        } else {
            lines.sort_by(|a, b| key(a).cmp(&key(b)));
        }
        if args.reverse {
            lines.reverse();
        }
        if args.unique {
            lines.dedup_by(|a, b| key(a) == key(b));
        }
        let old = last.saturating_sub(first).saturating_add(1);
        for (i, text) in lines.iter().enumerate() {
            self.buffer.replace_line_text(first.saturating_add(i), text);
        }
        for _ in lines.len()..old {
            self.buffer.delete_line(first.saturating_add(lines.len()));
        }
        self.text_location = Location { line_idx: first, grapheme_idx: 0 };
        self.seal_undo_group();
        self.set_needs_redraw(true);
        lines.len()
    }

    pub fn settle(&mut self) {
        self.start_line_fill();
        self.shown_row = self.scroll_offset.row;
        self.shown_col = self.scroll_offset.col;
        self.guide_anim = None;
        self.trail = None;
        self.comp_anim = None;
        self.guide_key = None;
        self.set_needs_redraw(true);
    }

}

impl UIComponent for View {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    fn set_size(&mut self, size: Size) {
        self.size = size;
        self.scroll_text_location_into_view();
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        self.buffer.refresh_syntax();
        self.sync_shown();
        let hidden = self.buffer.hidden_map();
        let cursor_col = self.text_location_to_position().col;
        if !self.h_free { self.scroll_horizontally(cursor_col); }
        let Size { height, width } = self.size;
        let end_y = origin_row.saturating_add(height);
        let top_third = height.div_ceil(3);
        let scroll_top = self.shown_row;

        let guides_on = self.scope_guide && !self.compact;

        let scope = if self.scope_highlight || self.scope_brackets {
            self.buffer.enclosing_pair(self.text_location)
        } else {
            None
        };
        let tint_scope = scope.filter(|_| self.scope_highlight);
        let bracket_scope = scope.filter(|_| self.scope_brackets);

        let mut guides = Guides {
            unit: if guides_on { self.buffer.indent_unit() } else { 4 },
            active: if guides_on {
                self.enclosing_scopes()
                    .into_iter()
                    .map(|(open, close)| {
                        (self.buffer.indent_end(open.line_idx), open.line_idx, close.line_idx)
                    })
                    .collect()
            } else {
                Vec::new()
            },
            reveal: None,
        };
        if guides_on {
            let active = guides.active.clone();
            guides.reveal = self.guide_reveal(&active);
        }

        let live_query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_deref());
        let query = live_query.or_else(|| {
            if self.hl_search { self.last_search.as_deref() } else { None }
        });
        let selected_match = live_query.is_some().then_some(self.text_location);
        let mut highlighter = Highlighter::new(
            query,
            selected_match,
            self.buffer.get_file_info().get_language().cloned(),
            self.buffer.ts_annotations(),
            self.selection_range(),
            tint_scope,
            bracket_scope,
        );
        let visible_total = self.buffer.height().saturating_sub(hidden.count_hidden());
        let (thumb_start, thumb_end) = Self::scrollbar_thumb_range(visible_total, height, scroll_top);

        if highlighter.needs_line_pass() {
            let last_needed = hidden.line_of_vrow(scroll_top.saturating_add(height));
            for current_row in 0..=last_needed.min(self.buffer.height()) {
                self.buffer.highlight(current_row, &mut highlighter);
            }
        }
        let gutter = self.gutter_width();
        let number_gutter = if self.compact { gutter } else { gutter.saturating_sub(2) };
        let show_scrollbar = width > 1 && visible_total > height && !self.compact;
        let text_width = self.text_width();
        let current_line = self.text_location.line_idx;
        let inside: Vec<usize> = if self.minimal_gutter {
            self.buffer.enclosing_fold_starts(current_line)
        } else {
            Vec::new()
        };
        let fill = self.fill_anim.map(|s| {
            let p = (s.elapsed().as_secs_f32() * 1000.0 / FILL_MS).min(1.0);
            1.0 - (1.0 - p) * (1.0 - p)
        });
        let selection = self.selection_range();
        let row_end = if show_scrollbar { width.saturating_sub(1) } else { width };
        for current_row in origin_row..end_y {
            let line_idx = hidden.line_of_vrow(
                current_row.saturating_sub(origin_row).saturating_add(scroll_top),
            );
            let left = self.shown_col;
            let right = left.saturating_add(text_width);
            if let Some(annotated_string) =
                self.buffer
                    .get_highlighted_substring(line_idx, left..right, &highlighter)
            {
                if gutter > 0 {
                    Terminal::print_line_number(current_row, line_idx, number_gutter, line_idx == current_line)?;
                    if !self.compact {
                        let (glyph, color) = match self.buffer.fold_state(line_idx) {
                            Some(true) => ("›", FOLD_CLOSED),
                            Some(false) if !self.minimal_gutter => ("⌄", FOLD_OPEN),
                            Some(false) if inside.first() == Some(&line_idx) => ("⌄", t::BLUE),
                            Some(false) if inside.contains(&line_idx) => ("⌄", FOLD_OPEN),
                            _ => (" ", FOLD_OPEN),
                        };
                        let cell_bg = if self.fold_hover == Some(line_idx) {
                            FOLD_HOVER_BG
                        } else {
                            Color::Reset
                        };
                        Terminal::print_styled_at(
                            Position { row: current_row, col: number_gutter },
                            glyph,
                            color,
                            cell_bg,
                            false,
                        )?;
                        Terminal::print_styled_at(
                            Position { row: current_row, col: number_gutter + 1 },
                            " ",
                            Color::Reset,
                            Color::Reset,
                            false,
                        )?;
                    }
                }
                let in_scope_body = tint_scope
                    .is_some_and(|(open, close)| line_idx > open.line_idx && line_idx < close.line_idx);
                let line_highlight = if line_idx == current_line {
                    let color = if in_scope_body {
                        Terminal::SCOPE_CURSOR_BG
                    } else {
                        Terminal::CURSOR_LINE_COLOR
                    };
                    let end = match fill {
                        Some(p) => gutter.saturating_add((row_end.saturating_sub(gutter) as f32 * p) as usize),
                        None => row_end,
                    };
                    Some((color, end)) 
                } else if in_scope_body {
                    Some((Terminal::SCOPE_BG, row_end))
                } else {
                    None
                };
                let selection_eol = selection
                    .is_some_and(|(start, end)| line_idx >= start.line_idx && line_idx < end.line_idx)
                    && {
                        let line_width = self
                            .buffer
                            .width_until(line_idx, self.buffer.grapheme_count(line_idx));
                        line_width >= left && line_width < right
                    };
                Terminal::print_annotated_row(
                    current_row,
                    gutter,
                    &annotated_string,
                    line_highlight,
                    selection_eol,
                )?;
                if guides_on {
                    let selected_cols = selection.and_then(|(start, end)| {
                        if line_idx < start.line_idx || line_idx > end.line_idx {
                            return None;
                        }
                        let from = if line_idx == start.line_idx { start.grapheme_idx } else { 0 };
                        let to = if line_idx == end.line_idx { end.grapheme_idx } else { usize::MAX };
                        Some((from, to))
                    });
                    self.draw_indent_guides(
                        &guides,
                        current_row,
                        line_idx,
                        gutter,
                        line_highlight.map_or(Color::Reset, |(color, _)| color),
                        selected_cols,
                    )?;
                }
                if let Some(count) = self.buffer.closed_fold_len(line_idx) {
                    let line_w = self
                        .buffer
                        .width_until(line_idx, self.buffer.grapheme_count(line_idx));
                    if line_w >= left {
                        let used = line_w - left;
                        let room = text_width.saturating_sub(used).saturating_sub(1);
                        let marker: String = format!(" ⋯ {count} lines ").chars().take(room).collect();
                        if !marker.is_empty() {
                            Terminal::print_styled_at(
                                Position {
                                    row: current_row,
                                    col: gutter.saturating_add(used).saturating_add(1),
                                },
                                &marker,
                                FOLD_FG,
                                FOLD_BG,
                                false,
                            )?;
                        }
                    }
                }
            } else if current_row == top_third && self.buffer.is_empty() {
                if gutter > 0 {
                    Terminal::print_filler_row(current_row, number_gutter)?;
                } else {
                    Self::render_line(current_row, "~")?;
                }
                let origin = gutter.max(1);
                let message = Self::build_welcome_message(width.saturating_sub(origin).saturating_sub(1));
                if !message.is_empty() {
                    Terminal::print_at(Position { row: current_row, col: origin }, &message)?;
                }
            } else if gutter > 0 {
                Terminal::print_filler_row(current_row, number_gutter)?;
            } else {
                Self::render_line(current_row, "~")?;
            }
            if show_scrollbar {
                let relative_row = current_row.saturating_sub(origin_row);
                let on_thumb = relative_row >= thumb_start && relative_row < thumb_end;
                let (symbol, color) = if on_thumb {
                    ("┃", SCROLL_THUMB)
                } else {
                    ("│", SCROLL_TRACK)
                };
                let _ = Terminal::print_styled_at(
                    Position { row: current_row, col: width.saturating_sub(1) },
                    symbol,
                    color,
                    Color::Reset,
                    false,
                );
            }
        }
        self.draw_completion(origin_row)?;
        Ok(())
    }
}







#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::ex;
    use crate::testutil::TempDir;

    fn view_with(dir: &TempDir, text: &str) -> View {
        let path = dir.write("f.txt", text.as_bytes());
        let mut view = View::default();
        view.set_size(Size { height: 20, width: 80 });
        view.load(path.to_str().unwrap()).unwrap();
        view
    }

    #[test]
    fn grapheme_classes() {
        assert_eq!(grapheme_class("a"), 0);
        assert_eq!(grapheme_class("_"), 0);
        assert_eq!(grapheme_class(" "), 1);
        assert_eq!(grapheme_class("-"), 2);
    }

    #[test]
    fn scrollbar_thumb() {
        assert_eq!(View::scrollbar_thumb_range(5, 10, 0), (0, 10));
        assert_eq!(View::scrollbar_thumb_range(100, 10, 0), (0, 1));
        assert_eq!(View::scrollbar_thumb_range(100, 10, 90), (9, 10));
    }

    #[test]
    fn fold_hover_and_click_share_one_hit_test() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\n  b\n  c\nd\n  e\n");
        view.buffer.fold_close_all(); // refresh (compute) folds
        view.buffer.fold_open_all();  // leave them open
        let col = view.gutter_width().saturating_sub(2);
        // A fold-start row hits at the marker column...
        assert_eq!(view.fold_cell_line(0, col), Some(0));
        // ...and a click toggles exactly that fold (not a cursor move).
        assert!(view.handle_mouse_press(0, col, 1));
        assert_eq!(view.buffer.fold_state(0), Some(true), "line 0 should be closed now");
        assert_eq!(view.buffer.fold_state(3), Some(false), "line 3 must be untouched");
        view.buffer.fold_toggle(0); // restore so later asserts see open folds
        // Non-start rows, adjacent gutter columns, and text columns never hit.
        assert_eq!(view.fold_cell_line(1, col), None);
        assert_eq!(view.fold_cell_line(0, col.saturating_sub(1)), None);
        assert_eq!(view.fold_cell_line(0, col.saturating_add(1)), None);
        assert_eq!(view.fold_cell_line(0, view.gutter_width().saturating_add(3)), None);
    }

    #[test]
    fn fold_hover_tracks_scroll() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\n  b\n  c\nd\n  e\n");
        view.buffer.fold_close_all(); // compute folds
        view.buffer.fold_open_all();
        let col = view.gutter_width().saturating_sub(2);
        view.scroll_view_down(); // scrolls 3 rows: screen row 0 is now buffer line 3
        assert_eq!(view.fold_cell_line(0, col), Some(3));
        // Click agreement after scrolling: toggles line 3, not the old cell's line.
        assert!(view.handle_mouse_press(0, col, 1));
        assert_eq!(view.buffer.fold_state(3), Some(true));
    }

    #[test]
    fn fold_hover_off_in_compact_or_without_line_numbers() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\n  b\n  c\nd\n  e\n");
        view.buffer.fold_close_all(); // compute folds
        view.buffer.fold_open_all();
        let col = view.gutter_width().saturating_sub(2);
        view.compact = true; // compact gutter draws no fold markers
        assert_eq!(view.fold_cell_line(0, col), None);
        view.compact = false;
        assert!(!view.toggle_line_numbers()); // line numbers off -> no gutter at all
        assert_eq!(view.fold_cell_line(0, col), None);
    }

    #[test]
    fn fold_hover_redraws_only_on_change() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\n  b\n  c\nd\n  e\n");
        view.set_needs_redraw(false);
        view.set_fold_hover(None); // same value -> no redraw requested
        assert!(!view.needs_redraw());
        view.set_fold_hover(Some(0));
        assert!(view.needs_redraw());
        view.set_needs_redraw(false);
        view.set_fold_hover(Some(0)); // unchanged -> still no redraw
        assert!(!view.needs_redraw());
    }

    #[test]
    fn fold_hover_cleared_by_toggle_and_scroll() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\n  b\n  c\nd\n  e\n");
        view.buffer.fold_close_all(); // compute folds
        view.buffer.fold_open_all();
        let col = view.gutter_width().saturating_sub(2);
        view.set_fold_hover(Some(0));
        assert!(view.handle_mouse_press(0, col, 1)); // toggling clears the hover
        assert_eq!(view.fold_hover, None);
        view.set_fold_hover(Some(0));
        view.scroll_view_down(); // scrolling clears the hover
        assert_eq!(view.fold_hover, None);
    }

    #[test]
    fn vertical_moves_remember_the_column() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "abcdef\nab\nabcdef\n");
        view.set_block_cursor(true);
        view.goto_position(Some(0), Some(5));
        view.handle_move_command(Move::Down);
        assert_eq!(view.cursor_location().grapheme_idx, 1);
        view.handle_move_command(Move::Down);
        assert_eq!(view.cursor_location().grapheme_idx, 5);
        view.handle_move_command(Move::Left);
        view.handle_move_command(Move::Up);
        view.handle_move_command(Move::Up);
        assert_eq!(view.cursor_location().grapheme_idx, 4);
    }

    #[test]
    fn insert_at_first_non_blank() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "  ab\n   \n");
        view.move_to_indent_for_insert();
        assert_eq!(view.cursor_location().grapheme_idx, 2);
        view.goto_position(Some(1), Some(0));
        view.move_to_indent_for_insert();
        assert_eq!(view.cursor_location().grapheme_idx, 3);
    }

    #[test]
    fn autopair_inserts_and_removes_both() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "\n");
        view.handle_edit_command(Edit::Insert('('));
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("()"));
        assert_eq!(view.cursor_location().grapheme_idx, 1);
        view.handle_edit_command(Edit::DeleteBackward);
        assert_eq!(view.buffer.line_content(0).as_deref(), Some(""));
    }

    #[test]
    fn smart_home_toggles() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "    foo bar\n");
        view.goto_position(Some(0), Some(8));
        view.handle_move_command(Move::SmartHome);
        assert_eq!(view.cursor_location().grapheme_idx, 4);
        view.handle_move_command(Move::SmartHome);
        assert_eq!(view.cursor_location().grapheme_idx, 0);
        view.handle_move_command(Move::SmartHome);
        assert_eq!(view.cursor_location().grapheme_idx, 4);
    }

    #[test]
    fn screen_motions_and_viewport_scroll() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, &"x\n".repeat(100));
        view.handle_move_command(Move::ScreenBottom);
        assert_eq!(view.cursor_line(), 19);
        view.handle_move_command(Move::ScreenTop);
        assert_eq!(view.cursor_line(), 0);

        // Scrolling past the cursor drags it along; otherwise it stays put.
        view.handle_move_command(Move::ViewDown);
        assert_eq!(view.scroll_offset.row, 1);
        assert_eq!(view.cursor_line(), 1);

        view.goto_line(50);
        let line = view.cursor_line();
        view.handle_move_command(Move::ViewTop);
        assert_eq!(view.scroll_offset.row, 50);
        assert_eq!(view.cursor_line(), line);
        view.handle_move_command(Move::ViewUp);
        assert_eq!(view.scroll_offset.row, 49);
        assert_eq!(view.cursor_line(), line);
    }

    #[test]
    fn typing_the_closer_steps_over_it() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "\n");
        view.handle_edit_command(Edit::Insert('('));
        view.handle_edit_command(Edit::Insert(')'));
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("()"));
        assert_eq!(view.cursor_location().grapheme_idx, 2);
    }

    #[test]
    fn autopairs_can_be_turned_off() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "\n");
        assert!(!view.toggle_autopairs());
        view.handle_edit_command(Edit::Insert('('));
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("("));
    }

    #[test]
    fn delete_line_then_paste() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "one\ntwo\nthree\n");
        view.handle_edit_command(Edit::DeleteLine);
        view.handle_edit_command(Edit::Paste);
        let lines: Vec<_> = (0..3).map(|i| view.buffer.line_content(i).unwrap()).collect();
        assert_eq!(lines, vec!["two", "one", "three"]);
        assert_eq!(view.cursor_line(), 1);
    }

    #[test]
    fn block_cursor_stays_on_a_character() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "abc\nd\n");
        view.set_block_cursor(true);
        view.handle_move_command(Move::EndOfLine);
        assert_eq!(view.cursor_location().grapheme_idx, 2);
        view.handle_move_command(Move::Right);
        assert_eq!(view.cursor_location().grapheme_idx, 2);
        view.handle_move_command(Move::Down);
        assert_eq!((view.cursor_line(), view.cursor_location().grapheme_idx), (1, 0));
    }

    #[test]
    fn mark_selection_includes_the_cursor_character() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "hello\n");
        view.set_block_cursor(true);
        view.start_selection();
        view.handle_select_command(Move::Right);
        view.handle_edit_command(Edit::YankSelection);
        assert_eq!(view.register_text(), "he");
    }

    #[test]
    fn yank_selection_clears_the_selection() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "hello\n");
        view.handle_select_command(Move::Right);
        assert!(view.selection_lines().is_some());
        view.handle_edit_command(Edit::YankSelection);
        assert!(view.selection_lines().is_none());
        assert_eq!(view.register_text(), "h");
    }

    #[test]
    fn substitute_counts_and_undoes() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a a a\nb\n");
        let sub = ex::parse_substitute("s/a/x/g").unwrap().unwrap();
        assert_eq!(view.substitute(0, 1, &sub), (3, 1));
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("x x x"));
        assert!(view.try_undo());
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("a a a"));
    }

    #[test]
    fn count_only_substitute_changes_nothing() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a a\n");
        let sub = ex::parse_substitute("s/a/x/gn").unwrap().unwrap();
        assert_eq!(view.substitute(0, 0, &sub), (2, 1));
        assert_eq!(view.buffer.line_content(0).as_deref(), Some("a a"));
    }

    #[test]
    fn goto_line_clamps() {
        let dir = TempDir::new();
        let mut view = view_with(&dir, "a\nb\nc\n");
        view.goto_line(999);
        assert_eq!(view.cursor_line(), 2);
        view.goto_line(0);
        assert_eq!(view.cursor_line(), 0);
    }
}