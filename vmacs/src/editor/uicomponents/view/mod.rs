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

#[derive(Default)]
pub struct View {
    buffer: Buffer,
    needs_redraw: bool,
    size: Size,
    text_location: Location,
    scroll_offset: Position,
    search_info: Option<SearchInfo>,
    register: Register, // Yank
}

impl View {
    pub fn get_status(&self) -> DocumentStatus {
        let file_info = self.buffer.get_file_info();
        DocumentStatus {
            total_lines: self.buffer.height(),
            current_line_idx: self.text_location.line_idx,
            file_name: format!("{file_info}"),
            is_modified: self.buffer.is_dirty(),
            file_type: file_info.get_file_type(),
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
            Edit::PasteInline => self.paste_inline(), // emacs
        }
    }
    pub fn handle_move_command(&mut self, command: Move) {
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
                at.grapheme_idx =
                    min(at.grapheme_idx.saturating_add(1), self.buffer.grapheme_count(at.line_idx));
            }
            for ch in self.register.content.chars() {
                self.buffer.insert_char(ch, at);
                at.grapheme_idx = at.grapheme_idx.saturating_add(1);
            }
            self.text_location = at;
        }
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn paste_inline(&mut self) {
        if self.register.content.is_empty() {
            return;
        }
        let mut at = self.text_location;
        for ch in self.register.content.chars() {
            self.buffer.insert_char(ch, at);
            at.grapheme_idx = at.grapheme_idx.saturating_add(1);
        }
        self.text_location = at;
        self.set_needs_redraw(true);
        self.scroll_text_location_into_view();
    }
    fn delete_to_line_end(&mut self) {
        self.buffer.delete_to_line_end(self.text_location);
        self.set_needs_redraw(true);
    }
    fn insert_newline(&mut self) {
        self.buffer.insert_newline(self.text_location);
        self.handle_move_command(Move::Right);
        self.set_needs_redraw(true);
    }
    fn delete_backward(&mut self) {
        if self.text_location.line_idx != 0 || self.text_location.grapheme_idx != 0 {
            self.handle_move_command(Move::Left);
            self.delete();
        }
    }
    fn delete(&mut self) {
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
        }
        self.set_needs_redraw(true);
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

    pub fn caret_position(&self) -> Position {
        self.text_location_to_position()
            .saturating_sub(self.scroll_offset)
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
            self.buffer.get_file_info().get_file_type(),
        );

        for current_row in 0..end_y.saturating_add(scroll_top) {
            self.buffer.highlight(current_row, &mut highlighter); 
        }
        for current_row in origin_row..end_y {
            let line_idx = current_row
                .saturating_sub(origin_row)
                .saturating_add(scroll_top);
            let left = self.scroll_offset.col;
            let right = self.scroll_offset.col.saturating_add(width);
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
        }
        Ok(())
    }
}