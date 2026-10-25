#![allow(
    clippy::as_conversions, clippy::cast_precision_loss, clippy::cast_possible_truncation,
    clippy::cast_sign_loss, clippy::arithmetic_side_effects, clippy::integer_division
)]

use std::{
    io::Error,
    time::{Duration, Instant},
};

use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::prelude::*;

use super::super::{theme as t, Terminal};

const ENTER_MS: f32 = 140.0;
const EXIT_MS: f32 = 160.0;
const MAX_W: usize = 46;
const MIN_W: usize = 22;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Success,
    Warn,
    Error,
}

impl Level {
    fn color(self) -> Color {
        match self {
            Self::Info => t::BLUE,
            Self::Success => t::GREEN,
            Self::Warn => t::AMBER,
            Self::Error => t::RED,
        }
    }
    const fn icon(self) -> &'static str {
        match self {
            Self::Info => "●",
            Self::Success => "✓",
            Self::Warn => "▲",
            Self::Error => "✗",
        }
    }
    const fn life(self) -> f32 {
        match self {
            Self::Info => 1.0,
            Self::Success => 0.8,
            Self::Warn => 1.4,
            Self::Error => 2.0,
        }
    }
}

pub fn classify(msg: &str) -> Option<(Level, &'static str)> {
    use Level::{Error, Info, Success, Warn};
    let e_code = msg.starts_with('E') && msg.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
    let starts = |p: &str| msg.starts_with(p);
    Some(if starts("WARNING") || starts("E37") || starts("E45") {
        (Warn, "Warning")
    } else if starts("E486") || starts("E35") {
        (Warn, "Search")
    } else if starts("Buffer is read-only") {
        (Warn, "Read-only")
    } else if starts("ERR") || starts("Error") || e_code {
        (Error, "Error")
    } else if starts("Reloaded") {
        (Success, "Config")
    } else if starts("Restored") {
        (Success, "Session")
    } else if starts("Replaced") || starts("Saved ") || starts("Reopened") {
        (Success, "Done")
    } else if starts("Already at") {
        (Info, "History")
    } else if starts("Buffer is writable") {
        (Info, "Writable")
    } else if starts("New file") {
        (Info, "New file")
    } else if starts("Pinned") || starts("Unpinned") || starts("Cleared") || starts("Sorted") {
        (Info, "Done")
    } else if starts("Will save") {
        (Info, "Save format")
    } else {
        return None;
    })
}

struct Toast {
    level: Level,
    title: String,
    body: String,
    born: Instant,
    renewed: Instant,
    life: Duration,
    count: u32,
    dying: Option<Instant>,
}

#[derive(Default)]
pub struct Notifier {
    toasts: Vec<Toast>,
    enabled: bool,
    animate: bool,
    max: usize,
    base_ms: u64,
    sig: u64,
}

fn clip(text: &str, max: usize) -> String {
    let (mut out, mut used) = (String::new(), 0);
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.width() + 1 + word.width() <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            *last = format!("{}…", clip(last, width.saturating_sub(1)));
        }
    }
    lines.into_iter().map(|l| clip(&l, width)).collect()
}

/// Prints `text` at (row, x), clipped to the screen. Returns the next x.
fn put(row: RowIdx, x: usize, text: &str, fg: Color, bg: Color, bold: bool, screen_w: usize) -> usize {
    let next = x + text.width();
    if x < screen_w {
        let shown = clip(text, screen_w - x);
        if !shown.is_empty() {
            let _ = Terminal::print_styled_at(Position { row, col: x }, &shown, fg, bg, bold);
        }
    }
    next
}

impl Notifier {
    pub fn configure(&mut self, enabled: bool, animate: bool, max: usize, base_ms: u64) {
        self.enabled = enabled;
        self.animate = animate;
        self.max = max;
        self.base_ms = base_ms;
        if !enabled {
            self.toasts.clear();
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.toasts.clear();
        }
    }

    pub fn push(&mut self, level: Level, title: &str, body: &str) {
        if !self.enabled {
            return;
        }
        if let Some(t) = self
            .toasts
            .iter_mut()
            .find(|t| t.dying.is_none() && t.level == level && t.title == title && t.body == body)
        {
            t.count += 1;
            t.renewed = Instant::now();
            self.sig = 0;
            return;
        }
        let live = self.toasts.iter().filter(|t| t.dying.is_none()).count();
        if live >= self.max.max(1) {
            if let Some(old) = self.toasts.iter_mut().find(|t| t.dying.is_none()) {
                old.dying = Some(Instant::now());
            }
        }
        let now = Instant::now();
        self.toasts.push(Toast {
            level,
            title: title.to_string(),
            body: body.to_string(),
            born: now,
            renewed: now,
            life: Duration::from_millis((self.base_ms as f32 * level.life()) as u64),
            count: 1,
            dying: None,
        });
        self.sig = 0;
    }

    fn enter(&self, t: &Toast) -> f32 {
        if self.animate { (t.born.elapsed().as_secs_f32() * 1000.0 / ENTER_MS).min(1.0) } else { 1.0 }
    }
    fn exit(&self, t: &Toast) -> f32 {
        match t.dying {
            Some(d) if self.animate => (d.elapsed().as_secs_f32() * 1000.0 / EXIT_MS).min(1.0),
            Some(_) => 1.0,
            None => 0.0,
        }
    }
    fn left(t: &Toast) -> f32 {
        (1.0 - t.renewed.elapsed().as_secs_f32() / t.life.as_secs_f32().max(0.001)).clamp(0.0, 1.0)
    }

    pub fn active(&self) -> bool {
        !self.toasts.is_empty()
    }

    pub fn animating(&self) -> bool {
        self.toasts.iter().any(|t| self.enter(t) < 1.0 || t.dying.is_some())
    }

    pub fn tick(&mut self) -> bool {
        for t in &mut self.toasts {
            if t.dying.is_none() && t.renewed.elapsed() >= t.life {
                t.dying = Some(Instant::now());
            }
        }
        let exit_ms = if self.animate { EXIT_MS } else { 0.0 };
        self.toasts
            .retain(|t| t.dying.map_or(true, |d| d.elapsed().as_secs_f32() * 1000.0 < exit_ms));
        let mut sig = self.toasts.len() as u64 + 1;
        for t in &self.toasts {
            let step = (self.enter(t) * 16.0) as u64 * 7
                + (self.exit(t) * 16.0) as u64 * 131
                + (Self::left(t) * 40.0) as u64
                + u64::from(t.count) * 997;
            sig = sig.wrapping_mul(31).wrapping_add(step);
        }
        let changed = sig != self.sig;
        self.sig = sig;
        changed
    }

    pub fn render(&self, top: RowIdx, area: Size) -> Result<(), Error> {
        if self.toasts.is_empty() || area.width < 40 || area.height < 8 {
            return Ok(());
        }
        let screen_w = area.width;
        let mut row = top + 1;
        for toast in &self.toasts {
            let lines = wrap(&toast.body, MAX_W - 4, 2);
            let title = if toast.count > 1 {
                format!("{} ×{}", toast.title, toast.count)
            } else {
                toast.title.clone()
            };
            let head_w = title.width() + 4;
            let inner = lines
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap_or(0)
                .max(head_w)
                .clamp(MIN_W - 4, MAX_W - 4)
                .min(screen_w.saturating_sub(8));
            let (w, h) = (inner + 4, lines.len() + 2);
            if row + h > top + area.height {
                break;
            }
            let e = self.enter(toast);
            let x_out = 1.0 - e;
            let ease_in = x_out * x_out * x_out;
            let x_exit = self.exit(toast);
            let slide = (((ease_in + x_exit * x_exit) * (w + 2) as f32) as usize).min(w + 2);
            let x = screen_w.saturating_sub(w + 1) + slide;
            let (color, bg) = (toast.level.color(), t::SURFACE_RAISED);

            let mut cx = put(row, x, "╭─ ", color, bg, false, screen_w);
            let label = format!("{} {}", toast.level.icon(), title);
            cx = put(row, cx, &label, color, bg, true, screen_w);
            let dashes = w.saturating_sub(3 + label.width() + 2);
            put(row, cx, &format!(" {}╮", "─".repeat(dashes + 1)), color, bg, false, screen_w);

            for (i, text) in lines.iter().enumerate() {
                let r = row + 1 + i;
                let cx = put(r, x, "│ ", color, bg, false, screen_w);
                let pad = inner.saturating_sub(text.width());
                let cx = put(r, cx, &format!("{text}{}", " ".repeat(pad)), t::TEXT, bg, false, screen_w);
                put(r, cx, " │", color, bg, false, screen_w);
            }

            let bottom = row + h - 1;
            let span = w.saturating_sub(2);
            let lit = (Self::left(toast) * span as f32).round() as usize;
            let cx = put(bottom, x, "╰", color, bg, false, screen_w);
            let cx = put(bottom, cx, &"─".repeat(lit), color, bg, false, screen_w);
            let cx = put(bottom, cx, &"─".repeat(span.saturating_sub(lit)), t::LINE_DIM, bg, false, screen_w);
            put(bottom, cx, "╯", color, bg, false, screen_w);
            row += h;
        }
        Ok(())
    }
}