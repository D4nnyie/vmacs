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
use std::sync::atomic::{AtomicU32, Ordering};
use super::theme as t;
use std::sync::atomic::AtomicUsize;

use super::AnnotatedString;
use super::AnnotationType;

static ORIGIN_COL: AtomicUsize = AtomicUsize::new(0);
static ORIGIN_ROW: AtomicUsize = AtomicUsize::new(0);

const LINE_NUMBER_COLOR: Color = t::GUTTER;
const CURRENT_LINE_NUMBER_COLOR: Color = t::BLUE;

static EDITOR_BG: AtomicU32 = AtomicU32::new(0);
static EDITOR_FG: AtomicU32 = AtomicU32::new(0);
const COLOR_SET: u32 = 0x0100_0000;

fn pack(rgb: Option<(u8, u8, u8)>) -> u32 {
    rgb.map_or(0, |(r, g, b)| {
        COLOR_SET | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    })
}

#[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
fn unpack(value: u32) -> Option<Color> {
    (value & COLOR_SET != 0).then(|| Color::Rgb {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    })
}

pub struct Terminal;

pub struct StatusSpan {
    pub col: ColIdx,
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Terminal {
    pub const CURSOR_LINE_COLOR: Color = t::CURSOR_LINE;
    pub const SCOPE_BG: Color = t::SCOPE_BG;
    pub const SCOPE_CURSOR_BG: Color = t::SCOPE_CURSOR_BG;

    pub fn set_origin(col: usize, row: usize) {
        ORIGIN_COL.store(col, Ordering::Relaxed);
        ORIGIN_ROW.store(row, Ordering::Relaxed);
    }
    pub fn set_cursor_underline() -> Result<(), Error> {
        Self::queue_command(SetCursorStyle::BlinkingUnderScore)
    }
    pub fn move_caret_to(position: Position) -> Result<(), Error> {
        let col = position.col.saturating_add(ORIGIN_COL.load(Ordering::Relaxed));
        let row = position.row.saturating_add(ORIGIN_ROW.load(Ordering::Relaxed));
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        Self::queue_command(MoveTo(col as u16, row as u16))
    }

    pub fn draw_box(top: RowIdx, col: ColIdx, w: usize, h: usize, title: &str, color: Color) -> Result<(), Error> {
        let inner = w.saturating_sub(2);
        let label = if title.is_empty() { String::new() } else { format!(" {title} ") };
        let head = format!("╭{label}{}╮", "─".repeat(inner.saturating_sub(label.width())));
        Self::print_styled_at(Position { row: top, col }, &head, color, Color::Reset, true)?;
        for i in 1..h.saturating_sub(1) {
            Self::print_styled_at(Position { row: top + i, col }, "│", color, Color::Reset, false)?;
            Self::print_styled_at(Position { row: top + i, col: col + w - 1 }, "│", color, Color::Reset, false)?;
        }
        let foot = format!("╰{}╯", "─".repeat(inner));
        Self::print_styled_at(Position { row: top + h - 1, col }, &foot, color, Color::Reset, false)
    }

    pub fn terminate() -> Result<(), Error> {
        Self::queue_command(ResetColor)?;
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
        Self::reset_color()?; 
        Self::queue_command(Clear(ClearType::All))?;
        Ok(())
    }

    pub fn print_at(position: Position, text: &str) -> Result<(), Error> {
        Self::move_caret_to(position)?;
        Self::print(text)?;
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
        Self::reset_color()?;
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
        Self::reset_color()?;
        let background = line_highlight.map(|(color, _)| color);
        let limit = line_highlight.map_or(usize::MAX, |(_, end)| end);
        let mut pos = col;
        annotated_string
            .into_iter()
            .try_for_each(|part| -> Result<(), Error> {
                let lit = pos < limit;
                if lit {
                    if let Some(color) = background {
                        Self::queue_command(SetBackgroundColor(color))?;
                    }
                }
                let mut has_bg = false;
                if let Some(annotation_type) = part.annotation_type {
                    let attribute: Attribute = annotation_type.into();
                    has_bg = attribute.background.is_some();
                    Self::set_attribute(&attribute)?;
                }
                if part.scoped && !has_bg {
                    let color = if line_highlight.is_some() && lit {
                        Self::SCOPE_CURSOR_BG
                    } else {
                        Self::SCOPE_BG
                    };
                    Self::queue_command(SetBackgroundColor(color))?;
                }
                Self::print(part.string)?;
                pos = pos.saturating_add(part.string.width());
                Self::reset_color()
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
        if let Some(fg) = unpack(EDITOR_FG.load(Ordering::Relaxed)) {
            Self::queue_command(SetForegroundColor(fg))?;
        }
        if let Some(bg) = unpack(EDITOR_BG.load(Ordering::Relaxed)) {
            Self::queue_command(SetBackgroundColor(bg))?;
        }
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
        Self::reset_color()
    }
    pub fn end_frame() -> Result<(), Error> {
        Self::queue_command(EndSynchronizedUpdate)?;
        Ok(())
    }
    pub fn set_cursor_blinkingbar() -> Result<(), Error> {
        Self::queue_command(SetCursorStyle::BlinkingBar)?;
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
        let bg = if bg == Color::Reset { Self::base_bg() } else { bg };
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

    pub fn set_theme(background: Option<(u8, u8, u8)>, foreground: Option<(u8, u8, u8)>) {
        EDITOR_BG.store(pack(background), Ordering::Relaxed);
        EDITOR_FG.store(pack(foreground), Ordering::Relaxed);
    }

    fn base_bg() -> Color {
        unpack(EDITOR_BG.load(Ordering::Relaxed)).unwrap_or(Color::Reset)
    }
}