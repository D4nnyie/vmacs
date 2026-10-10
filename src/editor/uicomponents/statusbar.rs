#![allow(clippy::arithmetic_side_effects, clippy::integer_division)]

use std::{
    io::Error,
    time::{Duration, Instant},
};

use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::prelude::*;

use super::super::{DocumentStatus, StatusSpan, Terminal};
use super::UIComponent;

const MESSAGE_DURATION: Duration = Duration::from_secs(5);

const BAR_BG: Color = Color::Rgb { r: 0x1a, g: 0x1e, b: 0x27 };
const TEXT: Color = Color::Rgb { r: 0xc5, g: 0xcf, b: 0xdb };
const SECONDARY: Color = Color::Rgb { r: 0x8f, g: 0x9b, b: 0xb0 };
const DIM: Color = Color::Rgb { r: 0x63, g: 0x75, b: 0x8a };
const SEPARATOR: Color = Color::Rgb { r: 0x3a, g: 0x45, b: 0x56 };
const BADGE_FG: Color = Color::Rgb { r: 0x11, g: 0x15, b: 0x1c };
const BLUE: Color = Color::Rgb { r: 0x82, g: 0xaa, b: 0xff };
const GREEN: Color = Color::Rgb { r: 0xc3, g: 0xe8, b: 0x8d };
const PURPLE: Color = Color::Rgb { r: 0xc7, g: 0x92, b: 0xea };
const YELLOW: Color = Color::Rgb { r: 0xf6, g: 0xc8, b: 0x5f };
const SALMON: Color = Color::Rgb { r: 0xf2, g: 0x8f, b: 0x79 };

const SEP: &str = " │ ";
const SEP_WIDTH: usize = 3;
const ARROW_RIGHT: &str = "\u{e0b0}";
const ARROW_LEFT: &str = "\u{e0b2}";
const CLOCK_BG: Color = PURPLE;

struct Item {
    text: String,
    fg: Color,
    priority: u8,
}

fn span(col: ColIdx, text: impl Into<String>, fg: Color) -> StatusSpan {
    StatusSpan { col, text: text.into(), fg, bg: BAR_BG, bold: false }
}

fn mode_color(mode: &str) -> Color {
    match mode {
        "INPUT" => GREEN,
        "MARK" => PURPLE,
        "HELP" => YELLOW,
        "BROWSE" => YELLOW,
        "REPLACE" => SALMON,
        _ => BLUE,
    }
}

fn message_color(message: &str) -> Color {
    let is_digit_after_e = message.starts_with('E')
        && message.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
    if message.starts_with("ERR")
        || message.starts_with("Error")
        || message.starts_with("WARNING")
        || is_digit_after_e
    {
        SALMON
    } else {
        YELLOW
    }
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

#[derive(Default)]
pub struct StatusBar {
    current_status: DocumentStatus,
    current_mode: String,
    message: Option<(String, Instant)>,
    needs_redraw: bool,
    size: Size,
    drawn_clock: String,
    clock_12h: bool,
}

impl StatusBar {
    pub fn update_status(&mut self, new_status: DocumentStatus) {
        if new_status != self.current_status {
            self.current_status = new_status;
            self.set_needs_redraw(true);
        }
    }
    pub fn update_mode(&mut self, mode: &str) {
        if mode != self.current_mode {
            self.current_mode = mode.to_string();
            self.set_needs_redraw(true);
        }
    }
    pub fn set_message(&mut self, text: &str) {
        self.message = if text.is_empty() {
            None
        } else {
            Some((text.to_string(), Instant::now()))
        };
        self.set_needs_redraw(true);
    }
    fn message_expired(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > MESSAGE_DURATION)
    }

    pub fn set_clock_12h(&mut self, on: bool) {
        if self.clock_12h != on {
            self.clock_12h = on;
            self.set_needs_redraw(true);
        }
    }

    fn clock_text(&self) -> String {
        let format = if self.clock_12h { "%I:%M %p" } else { "%H:%M" };
        chrono::Local::now().format(format).to_string()
    }
}

impl UIComponent for StatusBar {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw || self.message_expired() || self.clock_text() != self.drawn_clock
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        if self.message_expired() {
            self.message = None;
        }
        let clock = self.clock_text();
        self.drawn_clock = clock.clone();
        let width = self.size.width;
        let status = &self.current_status;
        let mut spans: Vec<StatusSpan> = Vec::new();

        let badge = format!(" {} ", self.current_mode);
        let badge_w = badge.width();
        spans.push(StatusSpan {
            col: 0,
            text: badge,
            fg: BADGE_FG,
            bg: mode_color(&self.current_mode),
            bold: true,
        });
        spans.push(StatusSpan {
            col: badge_w,
            text: ARROW_RIGHT.to_string(),
            fg: mode_color(&self.current_mode),
            bg: BAR_BG,
            bold: false,
        });
        let mut col = badge_w + 2;
        col += 1;

        let lang = status.file_type.as_str();
        spans.push(span(col, lang, PURPLE));
        col += lang.width() + 2;

        let name = truncate(&status.file_name, (width / 3).max(8));
        let name_w = name.width();
        spans.push(span(col, name, TEXT));
        col += name_w;
        if status.is_modified {
            spans.push(span(col, " ●", YELLOW));
            col += 2;
        }
        let lines_text = format!("  {}", status.line_count_to_string());
        let lines_w = lines_text.width();
        if col + lines_w <= width / 2 {
            spans.push(span(col, lines_text, DIM));
            col += lines_w;
        }
        let left_end = col;

        let clock_text = format!(" {} ", clock);
        let clock_w = clock_text.width();
        let clock_block = clock_w + 1; // +1 for the arrow
        let show_clock = left_end + clock_block + 4 <= width;
        let clock_block = if show_clock { clock_block } else { 0 };

        let items = vec![
            Item { text: status.position_indicator_to_string(), fg: TEXT, priority: 0 },
            Item { text: status.indent.to_string(), fg: SECONDARY, priority: 2 },
            Item { text: status.encoding.to_string(), fg: SECONDARY, priority: 4 },
            Item { text: status.line_ending.to_string(), fg: SECONDARY, priority: 3 },
        ];

        let message = self
            .message
            .as_ref()
            .map(|(text, _)| text.as_str())
            .filter(|text| !text.is_empty())
            .map(|text| truncate(text, width.saturating_sub(left_end).saturating_sub(4)));
        let msg_w = message.as_ref().map_or(0, |m| m.width());

        let budget = if msg_w > 0 {
            width.saturating_sub(left_end).saturating_sub(msg_w).saturating_sub(6)
        } else {
            width.saturating_sub(left_end).saturating_sub(2)
        }
        .saturating_sub(clock_block);

        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&i| items[i].priority);
        let mut keep = vec![false; items.len()];
        let mut used = 0;
        let mut count = 0;
        for i in order {
            let add = items[i].text.width() + if count > 0 { SEP_WIDTH } else { 0 };
            if used + add > budget {
                break;
            }
            keep[i] = true;
            used += add;
            count += 1;
        }

        let right_start = if count > 0 {
            width.saturating_sub(clock_block + used + 1)
        } else {
            width.saturating_sub(clock_block)
        };
        let mut rcol = right_start;
        let mut first = true;
        for (item, kept) in items.iter().zip(&keep) {
            if !*kept {
                continue;
            }
            if !first {
                spans.push(span(rcol, SEP, SEPARATOR));
                rcol += SEP_WIDTH;
            }
            first = false;
            spans.push(span(rcol, item.text.clone(), item.fg));
            rcol += item.text.width();
        }

        if show_clock {
            spans.push(StatusSpan {
                col: width.saturating_sub(clock_block),
                text: ARROW_LEFT.to_string(),
                fg: CLOCK_BG,
                bg: BAR_BG,
                bold: false,
            });
            spans.push(StatusSpan {
                col: width.saturating_sub(clock_w),
                text: clock_text,
                fg: BADGE_FG,
                bg: CLOCK_BG,
                bold: true,
            });
        }

        if let Some(message) = message {
            let lower = left_end + 2;
            let upper = right_start.saturating_sub(msg_w + 2).max(lower);
            let start = (width.saturating_sub(msg_w) / 2).clamp(lower, upper);
            spans.push(StatusSpan {
                col: start,
                fg: message_color(&message),
                text: message,
                bg: BAR_BG,
                bold: true,
            });
        }

        Terminal::print_status_row(origin_row, width, BAR_BG, TEXT, &spans)
    }
}





#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_adds_an_ellipsis() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell…");
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn message_colors() {
        assert_eq!(message_color("E492: Not an editor command"), SALMON);
        assert_eq!(message_color("Error: cannot open"), SALMON);
        assert_eq!(message_color("WARNING! unsaved"), SALMON);
        assert_eq!(message_color("File saved successfully."), YELLOW);
        assert_eq!(message_color("Elapsed"), YELLOW);
    }

    #[test]
    fn mode_colors() {
        assert_eq!(mode_color("INPUT"), GREEN);
        assert_eq!(mode_color("MARK"), PURPLE);
        assert_eq!(mode_color("CONTROL"), BLUE);
    }

    #[test]
    fn clock_formats() {
        let mut bar = StatusBar::default();
        let t = bar.clock_text();
        assert_eq!(t.len(), 5, "{t}");
        assert_eq!(t.as_bytes()[2], b':');

        bar.set_clock_12h(true);
        let t = bar.clock_text();
        assert!(t.ends_with("AM") || t.ends_with("PM"), "{t}");
    }
}