#![allow(clippy::integer_division, clippy::arithmetic_side_effects)]

use std::{env, io::Error};

use unicode_width::UnicodeWidthStr;

use crate::prelude::*;

use super::super::{AnnotatedString, AnnotationType, Terminal};
use super::UIComponent;

const LOGO: &[&str] = &[
    "██╗   ██╗███╗   ███╗ █████╗  ██████╗███████╗",
    "██║   ██║████╗ ████║██╔══██╗██╔════╝██╔════╝",
    "██║   ██║██╔████╔██║███████║██║     ███████╗",
    "╚██╗ ██╔╝██║╚██╔╝██║██╔══██║██║     ╚════██║",
    " ╚████╔╝ ██║ ╚═╝ ██║██║  ██║╚██████╗███████║",
    "  ╚═══╝  ╚═╝     ╚═╝╚═╝  ╚═╝ ╚═════╝╚══════╝",
];
const LOGO_WIDTH: usize = 44;

const MENU_WIDTH: usize = 38;
const MAX_RECENT: usize = 5;

const MENU: &[(&str, &str)] = &[
    ("New File", "n/i"),
    ("Open File", "e"),
    ("Help", "h"),
    ("Quit", "q"),
];

#[derive(Default)]
pub struct Dashboard {
    recent: Vec<(String, String)>,
    size: Size,
    needs_redraw: bool,
}

impl Dashboard {
    pub fn set_recent(&mut self, paths: Vec<String>) {
        self.recent = paths
            .into_iter()
            .take(MAX_RECENT)
            .map(|path| (Self::label_for(&path), path))
            .collect();
        self.set_needs_redraw(true);
    }

    pub fn recent_path(&self, index: usize) -> Option<String> {
        self.recent.get(index).map(|(_, path)| path.clone())
    }
    fn label_for(path: &str) -> String {
        let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok();
        let mut label = match &home {
            Some(home) if !home.is_empty()
                && path.starts_with(home.as_str())
                && path[home.len()..].starts_with(['/', '\\']) =>
            {
                format!("~{}", &path[home.len()..])
            }
            _ => path.to_string(),
        };
        let max = MENU_WIDTH.saturating_sub(6);
        if label.chars().count() > max {
            let tail: Vec<char> = label.chars().rev().take(max.saturating_sub(3)).collect();
            label = format!("...{}", tail.into_iter().rev().collect::<String>());
        }
        label
    }

    fn block_pad(&self) -> usize {
        self.size.width.saturating_sub(MENU_WIDTH) / 2
    }

    fn centered(&self, text: &str, kind: Option<AnnotationType>) -> AnnotatedString {
        let pad = self.size.width.saturating_sub(text.width()) / 2;
        let line = format!("{}{text}", " ".repeat(pad));
        let mut row = AnnotatedString::from(&line);
        if let Some(kind) = kind {
            row.add_annotation(kind, pad, line.len());
        }
        row
    }

    fn block_heading(&self, text: &str) -> AnnotatedString {
        let pad = self.block_pad();
        let line = format!("{}{text}", " ".repeat(pad));
        let mut row = AnnotatedString::from(&line);
        row.add_annotation(AnnotationType::Keyword, pad, line.len());
        row
    }

    fn menu_row(&self, left: &str, key: &str) -> AnnotatedString {
        let pad = self.block_pad();
        let gap = MENU_WIDTH
            .saturating_sub(left.width())
            .saturating_sub(key.width())
            .max(1);
        let line = format!("{}{left}{}{key}", " ".repeat(pad), " ".repeat(gap));
        let mut row = AnnotatedString::from(&line);
        row.add_annotation(
            AnnotationType::Number,
            line.len().saturating_sub(key.len()),
            line.len(),
        );
        row
    }

    fn assemble(&self, show_logo: bool, show_extras: bool) -> Vec<AnnotatedString> {
        let mut rows = Vec::new();
        if show_logo {
            for line in LOGO {
                rows.push(self.centered(line, Some(AnnotationType::Attribute)));
            }
            rows.push(AnnotatedString::default());
        }
        rows.push(self.centered(
            &format!("Versatile Modal And Command System"),
            Some(AnnotationType::Punctuation),
        ));
        rows.push(self.centered(
            &format!("{VERSION}"),
            Some(AnnotationType::Punctuation),
        ));
        rows.push(AnnotatedString::default());
        for (label, key) in MENU {
            rows.push(self.menu_row(label, key));
        }
        if show_extras && !self.recent.is_empty() {
            rows.push(AnnotatedString::default());
            rows.push(self.block_heading("Recent Files"));
            for (index, (label, _)) in self.recent.iter().enumerate() {
                rows.push(self.menu_row(label, &index.saturating_add(1).to_string()));
            }
        }
        if show_extras {
            rows.push(AnnotatedString::default());
            rows.push(self.centered(
                ":help lists every keybinding and command",
                Some(AnnotationType::Punctuation),
            ));
        }
        rows
    }
}

impl UIComponent for Dashboard {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let height = self.size.height;
        let mut rows = self.assemble(self.size.width >= LOGO_WIDTH.saturating_add(2), true);
        if rows.len() > height {
            rows = self.assemble(false, true);
        }
        if rows.len() > height {
            rows = self.assemble(false, false);
        }
        let top = height.saturating_sub(rows.len()) / 2;
        for offset in 0..height {
            let row = origin_row.saturating_add(offset);
            match offset.checked_sub(top).and_then(|index| rows.get(index)) {
                Some(line) => Terminal::print_annotated_row(row, 0, line, None, false)?,
                None => Terminal::print_row(row, "")?,
            }
        }
        Ok(())
    }
}