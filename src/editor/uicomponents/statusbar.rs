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
use super::super::theme as t;

const MESSAGE_DURATION: Duration = Duration::from_secs(5);

const BAR_BG: Color = t::SURFACE; const TEXT: Color = t::TEXT; const SECONDARY: Color = t::TEXT_MUTED;
const DIM: Color = t::TEXT_DIM; const SEPARATOR: Color = t::SEPARATOR; const BADGE_FG: Color = t::ON_ACCENT;
const BLUE: Color = t::BLUE; const GREEN: Color = t::GREEN; const PURPLE: Color = t::PURPLE;
const AMBER: Color = t::AMBER; const RED: Color = t::RED; const MODIFIED: Color = t::MODIFIED;

const SEP: &str = " │ ";
const SEP_WIDTH: usize = 3;
const ARROW_RIGHT: &str = "\u{e0b0}";
const ARROW_LEFT: &str = "\u{e0b2}";
const CLOCK_BG: Color = PURPLE;
const MODE_MS: f32 = 220.0;
const TINT_MS: f32 = 450.0;

struct Item {
    text: String,
    fg: Color,
    priority: u8,
}

fn span(col: ColIdx, text: impl Into<String>, fg: Color) -> StatusSpan {
    StatusSpan { col, text: text.into(), fg, bg: BAR_BG, bold: false }
}

fn mode_color(mode: &str) -> Color { t::mode_color(mode) }

fn message_color(message: &str) -> Color {
    let e_code = message.starts_with('E') && message.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
    if message.starts_with("ERR") || message.starts_with("Error") || e_code { RED }
    else if message.starts_with("WARNING") { AMBER }
    else if ["File saved", "Saved", "Replaced", "Pinned", "Unpinned", "Restored", "Reloaded"]
        .iter().any(|p| message.starts_with(p)) { GREEN }
    else { BLUE }
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

fn truncate_left(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for ch in text.chars().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        tail.push(ch);
        used += w;
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

fn short_mode(mode: &str) -> String {
    match mode {
        "CONTROL" => "CTL",
        "INPUT" => "INP",
        "MARK" => "MRK",
        "HELP" => "HLP",
        "BROWSE" => "BRW",
        "REPLACE" => "REP",
        other => other,
    }
    .to_string()
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
    compact: bool,
    full_path: bool,
    workstation: bool, 
    powerline: bool,
    animate: bool,
    mode_since: Option<Instant>,
    tint: Option<(Instant, Color)>,
}

impl StatusBar {
    pub fn set_compact(&mut self, on: bool) {
        if self.compact != on {
            self.compact = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_full_path(&mut self, on: bool) {
        if self.full_path != on {
            self.full_path = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_animate(&mut self, on: bool) {
        self.animate = on;
        if !on { self.mode_since = None; self.tint = None; }
    }

    pub fn flash(&mut self, color: Color) {
        if self.animate {
            self.tint = Some((Instant::now(), color));
            self.set_needs_redraw(true);
        }
    }

    pub fn animating(&self) -> bool {
        self.mode_since.is_some() || self.tint.is_some()
    }

    fn progress(since: Instant, ms: f32) -> f32 {
        (since.elapsed().as_secs_f32() * 1000.0 / ms).min(1.0)
    }

    pub fn set_powerline(&mut self, on: bool) {
        if self.powerline != on {
            self.powerline = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_workstation(&mut self, on: bool) {
        if self.workstation != on {
            self.workstation = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn update_status(&mut self, new_status: DocumentStatus) {
        if new_status != self.current_status {
            self.current_status = new_status;
            self.set_needs_redraw(true);
        }
    }
    pub fn update_mode(&mut self, mode: &str) {
        if mode != self.current_mode {
            self.current_mode = mode.to_string();
            if self.animate { self.mode_since = Some(Instant::now()); }
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

    pub fn has_message(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|(text, at)| !text.is_empty() && at.elapsed() <= MESSAGE_DURATION)
    }
}

impl UIComponent for StatusBar {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw || self.animating() || self.message_expired() || self.clock_text() != self.drawn_clock
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
        let compact = self.compact;
        let status = &self.current_status;
        let base = mode_color(&self.current_mode);
        let mut finished = false;
        let badge_bg = match self.mode_since {
            Some(s) => {
                let p = Self::progress(s, MODE_MS);
                finished |= p >= 1.0;
                t::mix(t::TEXT_BRIGHT, base, 1.0 - (1.0 - p) * (1.0 - p))
            }
            None => base,
        };
        let bar_bg = match self.tint {
            Some((s, c)) => {
                let p = Self::progress(s, TINT_MS);
                finished |= p >= 1.0;
                t::mix(BAR_BG, c, 0.30 * (1.0 - p))
            }
            None => BAR_BG,
        };
        let mut spans: Vec<StatusSpan> = Vec::new();

        let ws = self.workstation;
        let arrows = !compact && (!ws || self.powerline);

        let mode_text = if compact { short_mode(&self.current_mode) } else { self.current_mode.clone() };
        let badge = format!(" {mode_text} ");
        let badge_w = badge.width();
        spans.push(StatusSpan {
            col: 0,
            text: badge,
            fg: BADGE_FG,
            bg: badge_bg,
            bold: true,
        });
        let mut col;
        if arrows {
            spans.push(StatusSpan {
                col: badge_w,
                text: ARROW_RIGHT.to_string(),
                fg: badge_bg,
                bg: BAR_BG,
                bold: false,
            });
            col = badge_w + 3;
        } else {
            col = badge_w + 1;
        }

        if !compact {
            let lang = status.file_type.as_str();
            spans.push(span(col, lang, PURPLE));
            col += lang.width() + if compact { 1 } else { 2 };
        }

        let name = if self.full_path && !status.file_path.is_empty() {
            truncate_left(&status.file_path, (width / 2).max(8))
        } else {
            let share = if compact { width / 4 } else { width / 3 };
            truncate(&status.file_name, share.max(8))
        };
        let name_w = name.width();
        spans.push(span(col, name, TEXT));
        col += name_w;
        if status.read_only {
            spans.push(span(col, " [RO]", RED));
            col += 5;
        }
        if status.is_modified {
            spans.push(span(col, " ●", MODIFIED));
            col += 2;
        }
        if !compact {
            let lines_text = format!("  {}", status.line_count_to_string());
            let lines_w = lines_text.width();
            if col + lines_w <= width / 2 {
                spans.push(span(col, lines_text, DIM));
                col += lines_w;
            }
        }
        let left_end = col;

        let clock_text = if compact { format!(" {clock}") } else { format!(" {clock} ") };
        let clock_w = clock_text.width();
        let clock_block = if compact { clock_w } else { clock_w + 1 };
        let show_clock = left_end + clock_block + 4 <= width && !(compact && width < 100);
        let clock_block = if show_clock { clock_block } else { 0 };

        let items = if compact {
            vec![
                Item { text: status.position_compact(), fg: TEXT, priority: 0 },
                Item {
                    text: format!("{} {}", status.encoding, status.line_ending),
                    fg: SECONDARY,
                    priority: 2,
                },
            ]
        } else {
            vec![
                Item { text: status.position_indicator_to_string(), fg: TEXT, priority: 0 },
                Item { text: status.indent.to_string(), fg: SECONDARY, priority: 2 },
                Item { text: status.encoding.to_string(), fg: SECONDARY, priority: 4 },
                Item { text: status.line_ending.to_string(), fg: SECONDARY, priority: 3 },
            ]
        };

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
            if compact {
                spans.push(span(width.saturating_sub(clock_w), clock_text, DIM));
            } else {
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

        for s in &mut spans {
            if s.bg == BAR_BG { s.bg = bar_bg; }
        }
        if finished {
            if self.mode_since.is_some_and(|s| Self::progress(s, MODE_MS) >= 1.0) { self.mode_since = None; }
            if self.tint.is_some_and(|(s, _)| Self::progress(s, TINT_MS) >= 1.0) { self.tint = None; }
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
        assert_eq!(message_color("E492: Not an editor command"), RED);
        assert_eq!(message_color("Error: cannot open"), RED);
        assert_eq!(message_color("WARNING! unsaved"), AMBER);
        assert_eq!(message_color("File saved successfully."), GREEN);
        assert_eq!(message_color("Elapsed"), BLUE);
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