use std::{cmp::min, io::Error};

use crate::editor::RowIdx;
use crate::prelude::*;

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

#[derive(Default)]
pub struct View {
    buffer: Buffer,
    needs_redraw: bool,
    size: Size,
    text_location: Location,
    scroll_offset: Position,
    search_info: Option<SearchInfo>,
    register: Register, // Yank
    selection_anchor: Option<Location>,
}

impl View {
    pub fn get_status(&self) -> DocumentStatus {
        let file_info = self.buffer.get_file_info();
        DocumentStatus {
            total_lines: self.buffer.height(),
            current_line_idx: self.text_location.line_idx,
            file_name: format!("{file_info}"),
            is_modified: self.buffer.is_dirty(),
            file_type: file_info.file_type_name(),
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
        let query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_ref());

        debug_assert!(
            query.is_some(),
            "Attempting to search with malformed searchinfo present"
        );
        query
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
        self.set_needs_redraw(true);
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.buffer.save()?;
        self.set_needs_redraw(true);
        Ok(())
    }
    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        self.buffer.save_as(file_name)?;
        self.set_needs_redraw(true);
        Ok(())
    }

    pub fn handle_edit_command(&mut self, command: Edit) {
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
            }
            Edit::UppercaseSelection => self.change_case_selection(true),
            Edit::LowercaseSelection => self.change_case_selection(false),
            Edit::IndentSelection => self.indent_selection(true),
            Edit::DedentSelection => self.indent_selection(false),
            Edit::InsertIndent => self.insert_indent(),
        }
    }
    pub fn handle_move_command(&mut self, command: Move) {
        self.clear_selection();
        self.move_cursor(command);
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
            let a = (anchor.line_idx, anchor.grapheme_idx);
            let c = (self.text_location.line_idx, self.text_location.grapheme_idx);
            if a <= c { (anchor, self.text_location) } else { (self.text_location, anchor) }
        })
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
        let mut loc = at;
        for ch in text.chars() {
            if ch == '\n' {
                self.buffer.insert_newline(loc);
                loc = Location { line_idx: loc.line_idx.saturating_add(1), grapheme_idx: 0 };
            } else {
                self.buffer.insert_char(ch, loc);
                loc.grapheme_idx = loc.grapheme_idx.saturating_add(1);
            }
        }
        loc
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

    pub fn yank_selection_to_system_clipboard(&self) -> Option<String> {
        self.selection_text()
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
    fn build_welcome_message(width: usize) -> String {
        if width == 0 {
            return String::new();
        }
        let welcome_message = format!("{NAME} editor -- version {VERSION}");
        let len = welcome_message.len();
        let remaining_width = width.saturating_sub(1);
        if remaining_width < len {
            return "~".to_string();
        }
        format!("{:<1}{:^remaining_width$}", "~", welcome_message)
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
        let Size { width, .. } = self.size;
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
        let Size { height, width } = self.size;
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
        (row < self.size.height && col < self.size.width).then_some(Position { row, col })
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
        let col_target = col.saturating_add(self.scroll_offset.col);
        self.text_location = self.buffer.location_for_position(line_idx, col_target);
        self.snap_to_valid_grapheme();
        self.snap_to_valid_line();
    }

    pub fn handle_mouse_press(&mut self, row: RowIdx, col: ColIdx) {
        self.clear_selection();
        self.move_to_screen_position(row, col);
        self.set_needs_redraw(true);
    }

    pub fn handle_mouse_drag(&mut self, row: RowIdx, col: ColIdx) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.text_location);
        }
        self.move_to_screen_position(row, col);
        self.set_needs_redraw(true);
    }

    pub fn handle_mouse_release(&mut self, row: RowIdx, col: ColIdx) {
        if self.selection_anchor.is_some() {
            self.move_to_screen_position(row, col);
            if self.has_selection() {
                if let Some(text) = self.selection_text() {
                    self.register = Register { content: text, linewise: false };
                }
            } else {
                self.clear_selection();
            }
            self.set_needs_redraw(true);
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
        let Size { height, width } = self.size;
        let end_y = origin_row.saturating_add(height);
        let top_third = height.div_ceil(3);
        let scroll_top = self.scroll_offset.row;

        let query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_deref());
        let selected_match = query.is_some().then_some(self.text_location);
        let mut highlighter = Highlighter::new(
            query,
            selected_match,
            self.buffer.get_file_info().get_language(),
            self.buffer.ts_annotations(),    
            self.selection_range(),
        );
        let (thumb_start, thumb_end) = Self::scrollbar_thumb_range(self.buffer.height(), height, scroll_top);
        let show_scrollbar = width > 1;

        for current_row in 0..end_y.saturating_add(scroll_top) {
            self.buffer.highlight(current_row, &mut highlighter); 
        }
        for current_row in origin_row..end_y {
            let line_idx = current_row
                .saturating_sub(origin_row)
                .saturating_add(scroll_top);
            let left = self.scroll_offset.col;
            let right = self.scroll_offset.col.saturating_add(width.saturating_sub(1));
            if let Some(annotated_string) =
                self.buffer
                    .get_highlighted_substring(line_idx, left..right, &highlighter)
            {
                Terminal::print_annotated_row(current_row, &annotated_string)?;
            } else if current_row == top_third && self.buffer.is_empty() {
                Self::render_line(current_row, &Self::build_welcome_message(width))?;
            } else {
                Self::render_line(current_row, "~")?;
            }
            if show_scrollbar {
                let relative_row = current_row.saturating_sub(origin_row);
                let symbol = if relative_row >= thumb_start && relative_row < thumb_end { "█" } else { "│" };
                let _ = Terminal::print_at(Position { row: current_row, col: width.saturating_sub(1) }, symbol);
            }
            if highlighter.needs_line_pass() { 
                for current_row in 0..end_y.saturating_add(scroll_top) {
                    self.buffer.highlight(current_row, &mut highlighter);
                }
            }
        }
        Ok(())
    }
}