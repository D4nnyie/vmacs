use std::io::Error;

use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::prelude::*;

use super::super::config::IconMode;
use super::super::{StatusSpan, Terminal};
use super::dashboard::terminal_supports_icons;
use super::UIComponent;
use super::super::theme as t;
use std::time::Instant;
const FADE_MS: f32 = 260.0;

const BAR_BG: Color = t::SURFACE_DEEP; const TAB_BG: Color = t::SURFACE; const ACTIVE_BG: Color = t::SURFACE_ACTIVE;
const TEXT: Color = t::TEXT; const DIM: Color = t::TEXT_DIM; const LINE_DIM: Color = t::LINE_DIM;
const YELLOW: Color = t::MODIFIED; const BLUE: Color = t::BLUE;
const HOVER_BG: Color = t::SURFACE_FILTER;
const HOVER_CLOSE: Color = t::RED; const HOVER_CLOSE_FG: Color = t::ON_ACCENT;

const LANGS: &[(&str, &str, &str, (u8, u8, u8))] = &[
    ("Rust", "RS", "\u{e7a8}", (0xde, 0xa5, 0x84)),
    ("Python", "PY", "\u{e606}", (0xff, 0xd4, 0x3b)),
    ("C", "C", "\u{e61e}", (0x7a, 0xa2, 0xf7)),
    ("C++", "C+", "\u{e61d}", (0x6a, 0x9f, 0xd8)),
    ("C#", "C#", "\u{f81a}", (0x9b, 0x6b, 0xdf)),
    ("JavaScript", "JS", "\u{e74e}", (0xf0, 0xdb, 0x4f)),
    ("TypeScript", "TS", "\u{e628}", (0x31, 0x9b, 0xe6)),
    ("Go", "GO", "\u{e627}", (0x00, 0xad, 0xd8)),
    ("Java", "JV", "\u{e738}", (0xe7, 0x6f, 0x51)),
    ("Bash", "SH", "\u{e795}", (0x89, 0xe0, 0x51)),
    ("Lua", "LU", "\u{e620}", (0x51, 0x6b, 0xd4)),
    ("SQL", "SQ", "\u{e706}", (0xdd, 0xdd, 0xdd)),
    ("Markdown", "MD", "\u{e73e}", (0x8f, 0xb8, 0xd8)),
    ("HTML", "<>", "\u{e736}", (0xe4, 0x4d, 0x26)),
    ("TOML", "TM", "\u{e6b2}", (0xc0, 0x6a, 0x4a)),
    ("YAML", "YM", "\u{e6a8}", (0xe0, 0x6c, 0x75)),
    ("Dashboard", "⌂", "\u{f015}", (0x82, 0xaa, 0xff)),
    ("Text", "≡", "\u{f15c}", (0x8f, 0x9b, 0xb0)),
    ("JSON", "{}", "\u{e60b}", (0xf0, 0xdb, 0x4f)),
    ("CSS", "CS", "\u{e749}", (0x56, 0x9c, 0xd6)),
    ("Ruby", "RB", "\u{e739}", (0xcc, 0x34, 0x2d)),
    ("Kotlin", "KT", "\u{e634}", (0xa9, 0x7b, 0xff)),
    ("Makefile", "MK", "\u{e779}", (0x6d, 0x80, 0x86)),
    ("Dockerfile", "DK", "\u{f308}", (0x24, 0x96, 0xed)),
    ("INI", "IN", "\u{e615}", (0x8f, 0x9b, 0xb0)),
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum IconSet {
    #[default]
    None,
    Plain,
    Nerd,
}

fn lang_entry(lang: &str) -> (String, String, Color) {
    for &(name, plain, nerd, (r, g, b)) in LANGS {
        if name == lang {
            return (plain.to_string(), nerd.to_string(), Color::Rgb { r, g, b });
        }
    }
    let badge: String = lang.chars().take(2).collect::<String>().to_uppercase();
    (badge.clone(), badge, BLUE)
}

#[derive(Clone, PartialEq, Eq)]
struct Tab {
    title: String,
    dirty: bool,
    lang: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabHit {
    pub idx: usize,
    pub close: bool,
}

#[derive(Default)]
pub struct TabBar {
    tabs: Vec<Tab>,
    active: usize,
    icons: IconSet,
    size: Size,
    needs_redraw: bool,
    compact: bool,
    ws: bool,
    animate: bool,
    active_anim: Option<Instant>,
    hover: Option<TabHit>,
}

fn fit(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn span(col: usize, text: String, fg: Color, bg: Color, bold: bool) -> StatusSpan {
    StatusSpan { col, text, fg, bg, bold }
}

impl TabBar {

    pub fn set_compact(&mut self, on: bool) {
        if self.compact != on {
            self.compact = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_workstation(&mut self, on: bool) {
        if self.ws != on {
            self.ws = on;
            self.set_needs_redraw(true);
        }
    }

    pub fn set_animate(&mut self, on: bool) {
        self.animate = on;
        if !on { self.active_anim = None; }
    }

    fn active_bg(&self) -> Color {
        match self.active_anim {
            Some(s) => {
                let p = (s.elapsed().as_secs_f32() * 1000.0 / FADE_MS).min(1.0);
                t::mix(t::BLUE_MUTED, ACTIVE_BG, 1.0 - (1.0 - p) * (1.0 - p))
            }
            None => ACTIVE_BG,
        }
    }

    fn close_w(&self) -> usize {
        if self.compact { 2 } else { 3 }
    }

    fn title_of(&self, tab: &Tab) -> String {
        let title = if self.compact && tab.title == "Dashboard" { "Dash" } else { tab.title.as_str() };
        fit(title, if self.compact { 12 } else { 22 })
    }

    pub fn set_icons(&mut self, mode: IconMode) {
        self.icons = match mode {
            IconMode::Off => IconSet::None,
            IconMode::Nerd => IconSet::Nerd,
            IconMode::On => IconSet::Plain,
            IconMode::Auto if terminal_supports_icons() => IconSet::Plain,
            IconMode::Auto => IconSet::None,
        };
        self.set_needs_redraw(true);
    }

    pub fn set_tabs(&mut self, tabs: Vec<(String, bool, String)>, active: usize) {
        let tabs: Vec<Tab> = tabs
            .into_iter()
            .map(|(title, dirty, lang)| Tab { title, dirty, lang })
            .collect();
        if tabs != self.tabs || active != self.active {
            if self.animate && active != self.active && !self.tabs.is_empty() {
                self.active_anim = Some(Instant::now());
            }
            self.tabs = tabs;
            self.active = active;
            self.hover = None; // stale idx/close may no longer point at the same tab
            self.set_needs_redraw(true);
        }
    }

    fn icon_for(&self, tab: &Tab) -> Option<(String, Color)> {
        if self.compact {
            return None;
        }
        let (plain, nerd, color) = lang_entry(&tab.lang);
        match self.icons {
            IconSet::None => None,
            IconSet::Plain => Some((plain, color)),
            IconSet::Nerd => Some((nerd, color)),
        }
    }

    fn tab_width(&self, tab: &Tab) -> usize {
        let icon_w = self.icon_for(tab).map_or(0, |(glyph, _)| glyph.width() + 1);
        let dirty_w = if !tab.dirty { 0 } else if self.compact { 1 } else { 2 };
        1 + icon_w + self.title_of(tab).width() + dirty_w + self.close_w()
    }

    fn slots(&self) -> (Vec<(usize, usize)>, usize) {
        let mut x = 0;
        let mut out = Vec::new();
        for tab in &self.tabs {
            let w = self.tab_width(tab);
            out.push((x, x + w));
            x += w + 1;
        }
        let offset = out
            .get(self.active)
            .map_or(0, |&(_, end)| end.saturating_sub(self.size.width));
        (out, offset)
    }

    pub fn range(&self, idx: usize) -> Option<(usize, usize)> {
        let (slots, offset) = self.slots();
        slots
            .get(idx)
            .map(|&(s, e)| (s.saturating_sub(offset), e.saturating_sub(offset)))
    }

    pub fn hit(&self, col: usize) -> Option<TabHit> {
        let (slots, offset) = self.slots();
        let x = col.saturating_add(offset);
        let idx = slots.iter().position(|&(s, e)| x >= s && x < e)?;
        let (_, end) = *slots.get(idx)?;
        let close = x.saturating_add(self.close_w()) >= end;
        Some(TabHit { idx, close })
    }

    /// Update the hover target, requesting a redraw only when it changes.
    pub fn set_hover(&mut self, hover: Option<TabHit>) {
        if self.hover != hover {
            self.hover = hover;
            self.set_needs_redraw(true);
        }
    }

    pub fn animating(&self) -> bool { self.active_anim.is_some() }
}

impl UIComponent for TabBar {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw || self.active_anim.is_some()
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let width = self.size.width;
        let (slots, offset) = self.slots();
        let mut spans: Vec<StatusSpan> = Vec::new();
        let abg = self.active_bg();
        for (i, (tab, &(s, e))) in self.tabs.iter().zip(&slots).enumerate() {
            if e <= offset || s >= offset + width {
                continue;
            }
            let active = i == self.active;
            // Hover highlights: body gets a subtle tint; the close cell gets a
            // stronger one. On the active tab the close cell is the only
            // element that still needs to stand out (its body already does).
            let hover_body = self.hover.is_some_and(|h| h.idx == i);
            let hover_close = self.hover.is_some_and(|h| h.idx == i && h.close);
            let bg = if active { abg } else if hover_body { HOVER_BG } else if self.ws { BAR_BG } else { TAB_BG };
            let fg = if active { TEXT } else { DIM };

            let mut segments: Vec<(String, Color, bool, Color)> = vec![(" ".to_string(), fg, false, bg)];
            if let Some((glyph, color)) = self.icon_for(tab) {
                segments.push((glyph, color, false, bg));
                segments.push((" ".to_string(), fg, false, bg));
            }
            segments.push((self.title_of(tab), fg, active, bg));
            if tab.dirty {
                segments.push(((if self.compact { "●" } else { " ●" }).to_string(), YELLOW, false, bg));
            }
            // Close cell stand-out on hover regardless of the tab's base bg.
            segments.push((
                (if self.compact { " ×" } else { " × " }).to_string(),
                if hover_close { HOVER_CLOSE_FG } else { fg },
                false,
                if hover_close { HOVER_CLOSE } else { bg },
            ));

            // A tab partly scrolled off the left edge draws only its visible tail.
            let skip = offset.saturating_sub(s);
            let mut skipped = 0usize;
            let mut col = s.saturating_sub(offset);
            for (text, color, bold, seg_bg) in segments {
                let mut text = text;
                if skipped < skip {
                    let drop = (skip - skipped).min(text.chars().count());
                    skipped += drop;
                    text = text.chars().skip(drop).collect();
                    if text.is_empty() {
                        continue;
                    }
                }
                let w = text.width();
                spans.push(span(col, text, color, seg_bg, bold));
                col += w;
            }
            if !self.ws && e >= offset && e - offset < width {
                spans.push(span(e - offset, "▏".to_string(), DIM, BAR_BG, false));
            }
        }

        let hidden_left = slots.iter().filter(|&&(_, e)| e <= offset).count();
        let total = slots.last().map_or(0, |&(_, e)| e);
        if hidden_left > 0 {
            spans.push(span(0, format!("‹{hidden_left}"), BLUE, BAR_BG, true));
        }
        if total.saturating_sub(offset) > width {
            let hidden_right = slots.iter().filter(|&&(s, _)| s >= offset + width).count();
            let t = format!("{hidden_right}›");
            spans.push(span(width.saturating_sub(t.width()), t, BLUE, BAR_BG, true));
        }

        Terminal::print_status_row(origin_row, width, BAR_BG, TEXT, &spans)?;

        if self.size.height >= 2 {
            let line_row = origin_row + 1;
            Terminal::print_styled_at(
                Position { row: line_row, col: 0 },
                &"─".repeat(width),
                LINE_DIM,
                BAR_BG,
                false,
            )?;
            if let (Some(tab), Some(&(s, e))) = (self.tabs.get(self.active), slots.get(self.active)) {
                let accent = lang_entry(&tab.lang).2;
                let from = s.saturating_sub(offset);
                let to = e.saturating_sub(offset).min(width);
                if to > from {
                    Terminal::print_styled_at(
                        Position { row: line_row, col: from },
                        &"━".repeat(to - from),
                        accent,
                        BAR_BG,
                        false,
                    )?;
                }
            }
        }
        if self.active_anim.is_some_and(|s| s.elapsed().as_secs_f32() * 1000.0 >= FADE_MS) {
            self.active_anim = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(n_tabs: usize, width: usize) -> TabBar {
        let mut bar = TabBar::default();
        bar.set_size(Size { height: 1, width });
        let tabs = (0..n_tabs)
            .map(|i| (format!("buf{i}"), i % 2 == 0, "Rust".to_string()))
            .collect();
        bar.set_tabs(tabs, 0);
        bar
    }

    #[test]
    fn hit_maps_body_and_close() {
        let bar = bar(2, 80);
        let near_start = bar.hit(1).expect("hit inside tab 0");
        assert_eq!(near_start, TabHit { idx: 0, close: false });
        let (_, end) = bar.range(0).unwrap();
        assert_eq!(bar.hit(end - 1).unwrap(), TabHit { idx: 0, close: true });
        assert!(!bar.hit(end - 1 - 3).unwrap().close); // 3 == close_w() for non-compact
        assert!(bar.hit(end).is_none()); // past the slot boundary
    }

    #[test]
    fn set_hover_only_flags_redraw_on_change() {
        let mut bar = bar(1, 80);
        let h = Some(TabHit { idx: 0, close: false });
        bar.set_needs_redraw(false);
        bar.set_hover(h);
        assert!(bar.needs_redraw());
        bar.set_needs_redraw(false);
        bar.set_hover(h); // unchanged target: no redraw requested
        assert!(!bar.needs_redraw());
        bar.set_hover(Some(TabHit { idx: 0, close: true }));
        assert!(bar.needs_redraw());
    }

    #[test]
    fn set_tabs_clears_stale_hover() {
        let mut bar = bar(2, 80);
        bar.set_hover(Some(TabHit { idx: 1, close: true }));
        let tabs: Vec<(String, bool, String)> = vec![("only".to_string(), false, "Rust".to_string())];
        bar.set_tabs(tabs, 0);
        assert!(bar.hover.is_none());
    }

    #[test]
    fn unchanged_tabs_keep_hover() {
        let mut bar = bar(1, 80);
        bar.set_hover(Some(TabHit { idx: 0, close: false }));
        // Same tab state as bar(1, _): no change, hover must survive.
        let tabs: Vec<(String, bool, String)> = vec![("buf0".to_string(), true, "Rust".to_string())];
        bar.set_tabs(tabs, 0);
        assert_eq!(bar.hover, Some(TabHit { idx: 0, close: false }));
    }
}