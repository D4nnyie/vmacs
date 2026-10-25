use std::{env, io::Error};
use unicode_width::UnicodeWidthStr;
use crate::prelude::*;
use super::super::{theme as t, StatusSpan, Terminal};
use super::UIComponent;

#[derive(Default)]
pub struct Header {
    workspace: String,
    info: String,
    size: Size,
    needs_redraw: bool,
}

impl Header {
    pub fn refresh_workspace(&mut self) {
        self.workspace = env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().chars().take(24).collect()))
            .unwrap_or_default();
        self.set_needs_redraw(true);
    }
    pub fn set_info(&mut self, info: String) {
        if info != self.info { self.info = info; self.set_needs_redraw(true); }
    }
}

impl UIComponent for Header {
    fn set_needs_redraw(&mut self, v: bool) { self.needs_redraw = v; }
    fn needs_redraw(&self) -> bool { self.needs_redraw }
    fn set_size(&mut self, size: Size) { self.size = size; }

    fn draw(&mut self, row: RowIdx) -> Result<(), Error> {
        let width = self.size.width;
        let mut spans = vec![
            StatusSpan { col: 1, text: "◆".into(), fg: t::PURPLE, bg: t::SURFACE, bold: true },
            StatusSpan { col: 3, text: "vMACS".into(), fg: t::BLUE, bg: t::SURFACE, bold: true },
        ];
        let mut col = 8;
        if !self.workspace.is_empty() && width >= 60 {
            spans.push(StatusSpan { col, text: " │ ".into(), fg: t::SEPARATOR, bg: t::SURFACE, bold: false });
            col += 3;
            spans.push(StatusSpan { col, text: self.workspace.clone(), fg: t::TEXT_MUTED, bg: t::SURFACE, bold: false });
            col += self.workspace.width();
        }
        let label = if self.info.is_empty() { "workstation".to_string() } else { format!("{} · workstation", self.info) };
        let lw = label.width();
        if col + lw + 3 <= width {
            spans.push(StatusSpan { col: width - lw - 1, text: label, fg: t::TEXT_DIM, bg: t::SURFACE, bold: false });
        }
        Terminal::print_status_row(row, width, t::SURFACE, t::TEXT, &spans)
    }
}