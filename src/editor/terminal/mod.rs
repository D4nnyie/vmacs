mod attribute;
use crate::prelude::*;
use attribute::Attribute;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::cursor::{Hide, MoveTo, SetCursorStyle, Show};
use crossterm::style::{
    Attribute::{Bold, Italic, Reset},
    Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, DisableLineWrap, EnableLineWrap,
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, BeginSynchronizedUpdate, EndSynchronizedUpdate,
};
use crossterm::{queue, Command};
use std::io::{stdout, Error, Write};
use unicode_width::UnicodeWidthStr;

use super::AnnotatedString;
use super::AnnotationType;

const LINE_NUMBER_COLOR: Color = Color::Rgb { r: 0x45, g: 0x59, b: 0x73 };
const CURRENT_LINE_NUMBER_COLOR: Color = Color::Rgb { r: 0x82, g: 0xaa, b: 0xff };

pub struct Terminal;

pub struct StatusSpan {
    pub col: ColIdx,
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Terminal {
    pub const CURSOR_LINE_COLOR: Color = Color::Rgb { r: 0x2e, g: 0x30, b: 0x33 };
    pub const SCOPE_BG: Color = Color::Rgb { r: 0x1f, g: 0x29, b: 0x3d };
    pub const SCOPE_CURSOR_BG: Color = Color::Rgb { r: 0x2b, g: 0x34, b: 0x47 };
    pub fn terminate() -> Result<(), Error> {
        Self::disable_mouse_capture()?;
        Self::disable_bracketed_paste()?;
        Self::leave_alternate_screen()?;
        Self::enable_line_wrap()?;
        Self::show_caret()?;
        Self::execute()?;
        Self::reset_cursor_style()?;
        disable_raw_mode()?;
        Ok(())
    }
    pub fn initialize() -> Result<(), Error> {
        Self::enable_bracketed_paste()?;
        enable_raw_mode()?;
        Self::enter_alternate_screen()?;
        Self::disable_line_wrap()?;
        Self::clear_screen()?;
        Self::enable_mouse_capture()?;
        Self::execute()?;
        Ok(())
    }
    pub fn clear_screen() -> Result<(), Error> {
        Self::queue_command(Clear(ClearType::All))?;
        Ok(())
    }

    pub fn print_at(position: Position, text: &str) -> Result<(), Error> {
        Self::move_caret_to(position)?;
        Self::print(text)?;
        Ok(())
    }

    pub fn move_caret_to(position: Position) -> Result<(), Error> {
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        Self::queue_command(MoveTo(position.col as u16, position.row as u16))?;
        Ok(())
    }
    pub fn enter_alternate_screen() -> Result<(), Error> {
        Self::queue_command(EnterAlternateScreen)?;
        Ok(())
    }
    pub fn leave_alternate_screen() -> Result<(), Error> {
        Self::queue_command(LeaveAlternateScreen)?;
        Ok(())
    }
    pub fn hide_caret() -> Result<(), Error> {
        Self::queue_command(Hide)?;
        Ok(())
    }
    pub fn show_caret() -> Result<(), Error> {
        Self::queue_command(Show)?;
        Ok(())
    }
    pub fn disable_line_wrap() -> Result<(), Error> {
        Self::queue_command(DisableLineWrap)?;
        Ok(())
    }
    pub fn enable_line_wrap() -> Result<(), Error> {
        Self::queue_command(EnableLineWrap)?;
        Ok(())
    }
    pub fn set_title(title: &str) -> Result<(), Error> {
        Self::queue_command(SetTitle(title))?;
        Ok(())
    }
    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }
    pub fn print_row(row: RowIdx, line_text: &str) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: 0 })?;
        Self::print(line_text)?;
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        Ok(())
    }
    pub fn print_annotated_row(
        row: RowIdx,
        col: ColIdx,
        annotated_string: &AnnotatedString,
        line_highlight: Option<(Color, ColIdx)>,
        selection_eol: bool,
    ) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col })?;
        let background = line_highlight.map(|(color, _)| color);
        if let Some(color) = background {
            Self::queue_command(SetBackgroundColor(color))?;
        }
        annotated_string
            .into_iter()
            .try_for_each(|part| -> Result<(), Error> {
                let mut has_bg = false;
                if let Some(annotation_type) = part.annotation_type {
                    let attribute: Attribute = annotation_type.into();
                    has_bg = attribute.background.is_some();
                    Self::set_attribute(&attribute)?;
                }
                if part.scoped && !has_bg {
                    let color = if line_highlight.is_some() {
                        Self::SCOPE_CURSOR_BG
                    } else {
                        Self::SCOPE_BG
                    };
                    Self::queue_command(SetBackgroundColor(color))?;
                }
                Self::print(part.string)?;
                Self::reset_color()?;
                if let Some(color) = background {
                    Self::queue_command(SetBackgroundColor(color))?;
                }
                Ok(())
            })?;

        let mut printed = annotated_string.to_string().width();
        if selection_eol {
            let attribute: Attribute = AnnotationType::Selection.into();
            Self::set_attribute(&attribute)?;
            Self::print(" ")?;
            Self::reset_color()?;
            printed = printed.saturating_add(1);
        }
        if let Some((color, end_col)) = line_highlight {
            let pad = end_col.saturating_sub(col).saturating_sub(printed);
            if pad > 0 {
                Self::queue_command(SetBackgroundColor(color))?;
                Self::print(&" ".repeat(pad))?;
            }
        }
        Self::reset_color()?;
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        Ok(())
    }
    fn set_attribute(attribute: &Attribute) -> Result<(), Error> {
        if let Some(foreground_color) = attribute.foreground {
            Self::queue_command(SetForegroundColor(foreground_color))?;
        }
        if let Some(background_color) = attribute.background {
            Self::queue_command(SetBackgroundColor(background_color))?;
        }
        if attribute.bold {
            Self::queue_command(SetAttribute(Bold))?;
        }
        if attribute.italic {
            Self::queue_command(SetAttribute(Italic))?;
        }
        Ok(())
    }
    fn reset_color() -> Result<(), Error> {
        Self::queue_command(ResetColor)?;
        Ok(())
    }
    pub fn size() -> Result<Size, Error> {
        let (width_u16, height_u16) = size()?;
        #[allow(clippy::as_conversions)]
        let height = height_u16 as usize;
        #[allow(clippy::as_conversions)]
        let width = width_u16 as usize;
        Ok(Size { height, width })
    }
    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }

    fn queue_command<T: Command>(command: T) -> Result<(), Error> {
        queue!(stdout(), command)?;
        Ok(())
    }
    pub fn enable_mouse_capture() -> Result<(), Error> {
        Self::queue_command(EnableMouseCapture)?;
        Ok(())
    }
    pub fn disable_mouse_capture() -> Result<(), Error> {
        Self::queue_command(DisableMouseCapture)?;
        Ok(())
    }
    pub fn begin_frame() -> Result<(), Error> {
        Self::queue_command(BeginSynchronizedUpdate)?;
        Ok(())
    }
    pub fn end_frame() -> Result<(), Error> {
        Self::queue_command(EndSynchronizedUpdate)?;
        Ok(())
    }
    pub fn set_cursor_bar() -> Result<(), Error> {
        Self::queue_command(SetCursorStyle::SteadyBar)?;
        Ok(())
    }
    pub fn set_cursor_block() -> Result<(), Error> {
        Self::queue_command(SetCursorStyle::BlinkingBlock)?;
        Ok(())
    }
    pub fn reset_cursor_style() -> Result<(), Error> {
        Self::queue_command(SetCursorStyle::DefaultUserShape)?;
        Ok(())
    }

    pub fn print_line_number(
        row: RowIdx,
        line_idx: LineIdx,
        gutter: usize,
        is_current: bool,
    ) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: 0 })?;
        let number_width = gutter.saturating_sub(1);
        let text = format!("{:>number_width$} ", line_idx.saturating_add(1));
        let color = if is_current { CURRENT_LINE_NUMBER_COLOR } else { LINE_NUMBER_COLOR };
        Self::queue_command(SetForegroundColor(color))?;
        if is_current {
            Self::queue_command(SetAttribute(Bold))?;
        }
        Self::print(&text)?;
        Self::reset_color()
    }
    pub fn print_filler_row(row: RowIdx, gutter: usize) -> Result<(), Error> {
        Self::move_caret_to(Position { row, col: 0 })?;
        let number_width = gutter.saturating_sub(1);
        Self::queue_command(SetForegroundColor(LINE_NUMBER_COLOR))?;
        Self::print(&format!("{:>number_width$}", "~"))?;
        Self::reset_color()?;
        Self::queue_command(Clear(ClearType::UntilNewLine))?;
        Ok(())
    }

    pub fn print_styled_at(
        position: Position,
        text: &str,
        fg: Color,
        bg: Color,
        bold: bool,
    ) -> Result<(), Error> {
        Self::move_caret_to(position)?;
        Self::queue_command(SetBackgroundColor(bg))?;
        Self::queue_command(SetForegroundColor(fg))?;
        if bold {
            Self::queue_command(SetAttribute(Bold))?;
        }
        Self::print(text)?;
        if bold {
            Self::queue_command(SetAttribute(Reset))?;
        }
        Self::reset_color()
    }

    pub fn print_status_row(
        row: RowIdx,
        width: usize,
        bg: Color,
        fg: Color,
        spans: &[StatusSpan],
    ) -> Result<(), Error> {
        Self::print_styled_at(Position { row, col: 0 }, &" ".repeat(width), fg, bg, false)?;
        for span in spans {
            if span.col >= width {
                continue;
            }
            Self::print_styled_at(
                Position { row, col: span.col },
                &span.text,
                span.fg,
                span.bg,
                span.bold,
            )?;
        }
        Ok(())
    }

    pub fn enable_bracketed_paste() -> Result<(), Error> {
        Self::queue_command(EnableBracketedPaste)?;
        Ok(())
    }
    pub fn disable_bracketed_paste() -> Result<(), Error> {
        Self::queue_command(DisableBracketedPaste)?;
        Ok(())
    }
}