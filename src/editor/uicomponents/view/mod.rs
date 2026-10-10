use std::{cmp::min, io::Error};

use crate::editor::RowIdx;
use crate::prelude::*;
use crossterm::style::Color;

use super::super::{
    command::{Edit, Move},
    DocumentStatus, Line, Terminal,
};
use super::UIComponent;
mod buffer;
use buffer::Buffer;
mod searchdirection;
use searchdirection::SearchDirection;
mod register;
use register::Register;
mod highlighter;
use highlighter::Highlighter;
mod fileinfo;
use fileinfo::FileInfo;
mod searchinfo;
use searchinfo::SearchInfo;
mod tscache;
use unicode_segmentation::UnicodeSegmentation;
use super::super::ex::{replace_in_line, Substitute};

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
}

const SCROLL_TRACK: Color = Color::Rgb { r: 0x2e, g: 0x35, b: 0x42 };
const SCROLL_THUMB: Color = Color::Rgb { r: 0x5c, g: 0x70, b: 0x99 };
const SCOPE_GUIDE: Color = Color::Rgb { r: 0x5c, g: 0x70, b: 0x99 };

impl View {
    pub fn get_status(&self) -> DocumentStatus {
        let file_info = self.buffer.get_file_info();
        DocumentStatus {
            total_lines: self.buffer.height(),
            current_line_idx: self.text_location.line_idx,
            current_column: self.text_location.grapheme_idx,
            file_name: format!("{file_info}"),
            is_modified: self.buffer.is_dirty(),
            file_type: file_info.file_type_name(),
            encoding: "UTF-8", // files are read with read_to_string, so always UTF-8
            line_ending: self.buffer.line_ending_label(),
            indent: self.buffer.indent_style(),
        }
    }

    pub const fn is_file_loaded(&self) -> bool {
        self.buffer.is_file_loaded()
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
        self.set_needs_redraw(true);
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.seal_undo_group();
        self.buffer.save()?;
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
                    super::super::clipboard::copy_to_system_clipboard(&text);
                }
                self.clear_selection();
            }
            Edit::UppercaseSelection => self.change_case_selection(true),
            Edit::LowercaseSelection => self.change_case_selection(false),
            Edit::IndentSelection => self.indent_selection(true),
            Edit::DedentSelection => self.indent_selection(false),
            Edit::InsertIndent => self.insert_indent(),
            Edit::Undo => {
                self.try_undo();
            }
            Edit::Redo => {
                self.try_redo();
            }
        }
    }
    pub fn handle_move_command(&mut self, command: Move) {
        self.clear_selection();
        self.move_cursor(command);
        self.set_needs_redraw(true);
    }

    pub fn handle_select_command(&mut self, command: Move) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.text_location);
        }
        self.move_cursor(command);
        self.set_needs_redraw(true);
    }

    fn move_cursor(&mut self, command: Move) {
        let Size { height, .. } = self.size;
        match command {
            Move::Up => self.move_up(1),
            Move::Down => self.move_down(1),
            Move::Left => self.move_left(),
            Move::Right => self.move_right(),
            Move::PageUp => self.move_up(height.saturating_sub(1)),
            Move::PageDown => self.move_down(height.saturating_sub(1)),
            Move::StartOfLine => self.move_to_start_of_line(),
            Move::EndOfLine => self.move_to_end_of_line(),
            Move::WordForward => {
                self.text_location = self.buffer.find_next_word_start(self.text_location);
            }
            Move::WordBackward => {
                self.text_location = self.buffer.find_prev_word_start(self.text_location);
            }
            Move::BufferStart => {
                self.text_location = Location { line_idx: 0, grapheme_idx: 0 };
            }
            Move::BufferEnd => {
                self.text_location.line_idx = self.buffer.height().saturating_sub(1);
                self.snap_to_valid_grapheme();
            }
        }
        self.scroll_text_location_into_view();
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
            if a <= c { (anchor, cursor) } else { (cursor, anchor) }
        })
    }

    pub fn paste_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
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
            self.buffer.delete_range(start, end);
            self.text_location = start;
            self.clear_selection();
            self.set_needs_redraw(true);
        }
    }

    pub fn yank_selection_to_system_clipboard(&self) -> Option<String> {
        self.selection_text()
    } 

    fn yank_selection(&mut self) {
        if let Some((start, _)) = self.selection_range() {
            if let Some(text) = self.selection_text() {
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
            self.buffer.insert_line(insert_idx, &self.register.content);
            self.text_location = Location { line_idx: insert_idx, grapheme_idx: 0 };
        } else {
            let mut at = self.text_location;
            if !before {
                at.grapheme_idx = min(at.grapheme_idx.saturating_add(1), self.buffer.grapheme_count(at.line_idx));
            }
            let content = self.register.content.clone();
            self.text_location = self.insert_text_at(at, &content);
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
            self.handle_move_command(Move::Left);
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
        let row = pos.row.checked_sub(self.scroll_offset.row)?;
        let col = pos.col.checked_sub(self.scroll_offset.col)?;
        (row < self.size.height && col < self.text_width()).then_some(Position {
            row,
            col: col.saturating_add(self.gutter_width()),
        })
    }

    fn text_location_to_position(&self) -> Position {
        let row = self.text_location.line_idx;
        debug_assert!(row.saturating_sub(1) <= self.buffer.height());
        let col = self
            .buffer
            .width_until(row, self.text_location.grapheme_idx);
        Position { col, row }
    }

    fn move_up(&mut self, step: usize) {
        self.text_location.line_idx = self.text_location.line_idx.saturating_sub(step);
        self.snap_to_valid_grapheme();
    }
    fn move_down(&mut self, step: usize) {
        self.text_location.line_idx = self.text_location.line_idx.saturating_add(step);
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }
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

    fn snap_to_valid_grapheme(&mut self) {
        self.text_location.grapheme_idx = min(
            self.text_location.grapheme_idx,
            self.buffer.grapheme_count(self.text_location.line_idx),
        );
    }

    fn snap_to_valid_line(&mut self) {
        self.text_location.line_idx = min(self.text_location.line_idx, self.buffer.height());
    }

    pub fn move_to_screen_position(&mut self, row: RowIdx, col: ColIdx) {
        let line_idx = row.saturating_add(self.scroll_offset.row);
        let col_target = col
            .saturating_sub(self.gutter_width())
            .saturating_add(self.scroll_offset.col);
        self.text_location = self.buffer.location_for_position(line_idx, col_target);
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }

    pub fn handle_mouse_press(&mut self, row: RowIdx, col: ColIdx, clicks: u8) {
        self.clear_selection();
        self.move_to_screen_position(row, col);
        self.click_unit = match clicks {
            2 => ClickUnit::Word,
            3 => ClickUnit::Line,
            _ => ClickUnit::Char,
        };
        self.click_span = None;
        if self.click_unit != ClickUnit::Char {
            let (start, end) = self.unit_span(self.text_location, self.click_unit);
            self.click_span = Some((start, end));
            self.selection_anchor = Some(start);
            self.text_location = end;
            self.scroll_text_location_into_view();
        }
        self.set_needs_redraw(true);
    }

    pub fn handle_mouse_drag(&mut self, row: RowIdx, col: ColIdx, step: usize) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.text_location);
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
        self.set_needs_redraw(true);
    }
    pub fn scroll_view_down(&mut self) {
        const STEP: usize = 3;
        let max_row = self.buffer.height().saturating_sub(1);
        self.scroll_offset.row = self.scroll_offset.row.saturating_add(STEP).min(max_row);
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

    fn guide_width(&self) -> usize {
        usize::from(self.scope_guide)
    }

    fn gutter_width(&self) -> usize {
        if self.no_line_numbers {
            return 0;
        }
        let digits = self.buffer.height().max(1).to_string().len();
        let gutter = digits
            .max(3)
            .saturating_add(1)
            .saturating_add(self.guide_width());
        if self.size.width > gutter.saturating_add(2) { gutter } else { 0 }
    }

    fn text_width(&self) -> usize {
        self.size
            .width
            .saturating_sub(self.gutter_width())
            .saturating_sub(1)
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

    pub fn clear(&mut self) {
        let size = self.size;
        let (no_autopairs, no_line_numbers) = (self.no_autopairs, self.no_line_numbers);
        let (scope_guide, scope_highlight, scope_brackets) =
            (self.scope_guide, self.scope_highlight, self.scope_brackets);
        *self = Self::default();
        self.size = size;
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

    pub fn clear_search_highlight(&mut self) {
        self.hl_search = false;
        self.set_needs_redraw(true);
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
        self.seal_undo_group();
        self.clear_selection();
        self.text_location = Location {
            line_idx: line_idx.min(self.buffer.height().saturating_sub(1)),
            grapheme_idx: 0,
        };
        self.reveal_location();
        self.set_needs_redraw(true);
    }

    fn reveal_location(&mut self) {
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
        let len = Line::from(pattern).grapheme_count();
        self.selection_anchor = Some(start);
        self.text_location = Location {
            line_idx: start.line_idx,
            grapheme_idx: start.grapheme_idx.saturating_add(len),
        };
        self.scroll_text_location_into_view();
        self.set_needs_redraw(true);
        Some(start)
    }

    pub fn qr_replace(&mut self, pattern: &str, with: &str, start: Location) -> Location {
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
        let cursor_col = self.text_location_to_position().col;
        self.scroll_horizontally(cursor_col);
        let Size { height, width } = self.size;
        let end_y = origin_row.saturating_add(height);
        let top_third = height.div_ceil(3);
        let scroll_top = self.scroll_offset.row;

        let scope = if self.scope_guide || self.scope_highlight || self.scope_brackets {
            self.buffer.enclosing_pair(self.text_location)
        } else {
            None
        };
        let guide_scope = scope.filter(|_| self.scope_guide);
        let tint_scope = scope.filter(|_| self.scope_highlight);
        let bracket_scope = scope.filter(|_| self.scope_brackets);

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
        let (thumb_start, thumb_end) = Self::scrollbar_thumb_range(self.buffer.height(), height, scroll_top);
        let show_scrollbar = width > 1 && self.buffer.height() > height;

        if highlighter.needs_line_pass() {
            for current_row in 0..end_y.saturating_add(scroll_top) {
                self.buffer.highlight(current_row, &mut highlighter);
            }
        }
        let gutter = self.gutter_width();
        let number_gutter = gutter.saturating_sub(self.guide_width());
        let text_width = self.text_width();
        let current_line = self.text_location.line_idx;
        let selection = self.selection_range();
        let row_end = if show_scrollbar { width.saturating_sub(1) } else { width };
        for current_row in origin_row..end_y {
            let line_idx = current_row
                .saturating_sub(origin_row)
                .saturating_add(scroll_top);
            let left = self.scroll_offset.col;
            let right = left.saturating_add(text_width);
            if let Some(annotated_string) =
                self.buffer
                    .get_highlighted_substring(line_idx, left..right, &highlighter)
            {
                if gutter > 0 {
                    Terminal::print_line_number(current_row, line_idx, number_gutter, line_idx == current_line)?;
                    if self.scope_guide {
                        let symbol = match guide_scope {
                            Some((open, close)) if open.line_idx != close.line_idx => {
                                if line_idx == open.line_idx {
                                    "╭"
                                } else if line_idx == close.line_idx {
                                    "╰"
                                } else if line_idx > open.line_idx && line_idx < close.line_idx {
                                    "│"
                                } else {
                                    " "
                                }
                            }
                            _ => " ",
                        };
                        Terminal::print_styled_at(
                            Position { row: current_row, col: number_gutter },
                            symbol,
                            SCOPE_GUIDE,
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
                    Some((color, row_end))
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