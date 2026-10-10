use std::io::Error;

use crate::prelude::*;

use super::super::{AnnotatedString, AnnotationType, StatusSpan, Terminal};
use super::UIComponent;

use crossterm::style::Color;
use unicode_width::UnicodeWidthStr;

const BAR_BG: Color = Color::Rgb { r: 0x1a, g: 0x1e, b: 0x27 };
const TEXT: Color = Color::Rgb { r: 0xc5, g: 0xcf, b: 0xdb };
const DIM: Color = Color::Rgb { r: 0x63, g: 0x75, b: 0x8a };
const KEY: Color = Color::Rgb { r: 0x82, g: 0xaa, b: 0xff };
const BADGE_FG: Color = Color::Rgb { r: 0x11, g: 0x15, b: 0x1c };
const BADGE_BG: Color = Color::Rgb { r: 0x8b, g: 0x92, b: 0x9e };

const HINTS: &[(&str, &str)] = &[
    ("j/k", "scroll"),
    ("PgUp/PgDn", "page"),
    ("g/G", "top/bottom"),
    ("q/Esc", "close"),
];


enum HelpRow {
    Heading(&'static str),
    Entry(&'static str, &'static str),
    Text(&'static str),
    Blank,
}

use HelpRow::{Blank, Entry, Heading, Text};

const HELP: &[HelpRow] = &[
    Heading("vMACS - keybindings and commands"),
    Text("Scroll: j/k, arrows, PgUp/PgDn, g/G.   Close: q or Esc."),
    Blank,
    Heading("MODES"),
    Entry("CONTROL", "default mode: vim-style motions and operators"),
    Entry("INPUT", "type text (enter with i a I A o O); Esc goes back"),
    Entry("MARK", "select text (enter with v); v or Esc goes back"),
    Blank,
    Heading("CONTROL MODE: MOTIONS"),
    Entry("h  j  k  l", "left, down, up, right"),
    Entry("0  $", "start / end of line"),
    Entry("w  b", "next / previous word"),
    Entry("gg  G", "start / end of buffer"),
    Entry("Arrows, PgUp, PgDn", "move (Home / End: start / end of line)"),
    Entry("C-Home / End", "start / end of buffer"),
    Blank,
    Heading("CONTROL MODE: EDITING"),
    Entry("i  a", "insert before / after the cursor"),
    Entry("I  A", "insert at line start / end"),
    Entry("o  O", "open a new line below / above"),
    Entry("x", "delete the character under the cursor"),
    Entry("dd", "delete the line"),
    Entry("dw  d$  D", "delete to the end of the line"),
    Entry("yy", "yank (copy) the line"),
    Entry("p  P", "paste after / before the cursor"),
    Entry("cc ", "copy selection to system clipboard"),
    Entry("u", "undo"),
    Entry("C-r / rr", "redo"),
    Entry("v", "enter Mark mode"),
    Entry(":", "open the command prompt"),
    Blank,
    Heading("INPUT MODE"),
    Entry("Esc", "back to Control mode"),
    Entry("Enter", "new line (keeps / adjusts indentation)"),
    Entry("Tab", "indent to the next 4-column stop"),
    Entry("Backspace, C-h", "delete before the cursor"),
    Entry("Delete, C-d", "delete under the cursor"),
    Entry("C-k", "delete to the end of the line"),
    Entry("C-y", "paste the register"),
    Entry("C-a  C-e", "start / end of line"),
    Entry("C-f  C-b", "right / left"),
    Entry("C-n  C-p", "down / up"),
    Entry("M-f  M-b", "next / previous word"),
    Entry("M-<  M->", "start / end of buffer"),
    Entry("Shift+Arrows", "select (Shift+Home / End: to line start / end)"),
    Entry("M-w", "yank the selection"),
    Entry("M-c", "copy the selection to the system clipboard"),
    Entry("Ctrl-Shift-v", "paste from the system clipboard"),
    Entry("C-z  C-r", "undo / redo"),
    Text("All Emacs-style keys work here."),
    Text("Typing in Input mode is one undo step until Esc or a cursor move."),
    Blank,
    Heading("MARK MODE"),
    Entry("h  j  k  l", "extend the selection"),
    Entry("0  $", "extend to start / end of line"),
    Entry("w  b", "extend by word"),
    Entry("g  G", "extend to start / end of buffer"),
    Entry("Arrows, PgUp, PgDn", "extend the selection"),
    Entry("d  x  Backspace", "delete the selection"),
    Entry("y", "yank the selection"),
    Entry("c", "copy the selection to the system clipboard"),
    Entry("u  U", "lowercase / UPPERCASE the selection"),
    Entry(">  <", "indent / dedent the selected lines"),
    Entry("v, Esc", "leave Mark mode"),
    Blank,
    Heading("EMACS-STYLE KEYS (Control and Input mode)"),
    Entry("C- M-", "ctrl / meta(alt)"),
    Entry("C-s", "search"),
    Entry("C-x", "save (asks for a name if the file has none)"),
    Entry("C-w", "save as"),
    Entry("C-q", "quit (unsaved changes: press it 3 times)"),
    Entry("C-a  C-e", "start / end of line"),
    Entry("C-f  C-b", "right / left"),
    Entry("C-n  C-p", "down / up"),
    Entry("C-d  C-h", "delete forward / backward"),
    Entry("C-k  C-y", "delete to end of line / paste"),
    Entry("M-f  M-b", "next / previous word"),
    Entry("M-w", "yank the selection"),
    Entry("M-g", "go to line (:goto N, :N also work)"),
    Entry("M-c", "copies to the system clipboard"),
    Entry("M-<  M->", "start / end of buffer"),
    Entry("M-h  or  F1", "this help screen"),
    Entry("C-s  C-r", "in search: next / previous match; C-g cancels"),
    Entry("M-%  :qr", "query replace (y n ! . q)"),
    Entry("M-x", "open the command prompt"),
    Blank,
    Heading("SEARCH (C-s)"),
    Entry("type", "search as you type"),
    Entry("Enter", "accept the current match"),
    Entry("Esc", "cancel and go back to where you were"),
    Entry("Right / Down", "next match"),
    Entry("Left / Up", "previous match"),
    Blank,
    Heading("SAVE PROMPT (C-x without a file name, C-w)"),
    Entry("Enter", "confirm the file name"),
    Entry("Esc", "abort"),
    Blank,
    Heading("COMMANDS (type : in Control mode)"),
    Entry("Tab / Shift-Tab", "list all commands, cycle through them"),
    Entry("Tab / → in :e :w", "complete paths; → accepts the entry and lists its contents"),
    Entry("Up / Down", "in : and \\ prompts: previous / next command"),
    Text("Suggestions are ordered by how often you use each command."),
    Entry(":w [file]", "save, optionally under a new name"),
    Entry(":q", "quit (refuses if there are unsaved changes)"),
    Entry(":q!", "quit without saving"),
    Entry(":wq [file]  :x", "save and quit"),
    Entry(":u  :undo", "undo"),
    Entry(":red  :redo", "redo"),
    Entry(":nu  :number", "toggle line numbers"),
    Entry(":ap  :autopairs", "toggle auto-pairs"),
    Entry(":h  :help", "show this screen"),
    Entry(":dash  :dash!", "back to the dashboard (! discards unsaved changes)"),
    Entry(":e  :e dir", "browse for a file (j/k, Enter, h, / filter, . hidden)"),
    Entry(":e! file", "open a file, discarding unsaved changes"),
    Entry("/  ?  n  N", "search forward / backward, next / previous match"),
    Entry("*  #", "search the word under the cursor"),
    Entry(":%s/a/b/g", "substitute (flags g i I n; ranges: 10,20  .,$  '<,'>)"),
    Entry(":42  :noh", "go to line 42 / clear search highlight"),
    Blank,
    Heading("CUSTOM COMMANDS (type \\ in Control mode)"),
    Text("Define them under [commands] in :config, then :reload. Each name maps"),
    Text("to a : command or a list of them. {} is replaced by typed arguments."),
    Entry("\\name [args]", "run a custom command (Tab completes names)"),
    Text("to a : command or a list of them. {} is replaced by all typed arguments,"),
    Text("{1} {2} ... by the individual ones (use quotes for args with spaces)."),
    Blank,
    Heading("MOUSE"),
    Entry("Click", "move the cursor"),
    Entry("Drag", "select text (also copied to the register)"),
    Entry("Scroll wheel", "scroll the view"),
    Entry("Double / triple", "select word / line (drag to extend)"),
    Entry("Middle click", "copy the selection to the system clipboard"),
    Blank,
    Heading("EDITING FEATURES"),
    Text("Auto-pairs: ( [ { \" ' ` insert their closer. Typing the closer steps"),
    Text("over it, Backspace in an empty pair removes both, and Enter inside {}"),
    Text("splits it onto three lines."),
    Text("Autoindent: Enter, o and O indent after { ( [ : and similar; a closing"),
    Text("bracket typed at the start of a line snaps back to its opener."),
    Text("Syntax: tree-sitter for Rust, Python, JS, TS, C, C++, Go, Java, Bash;"),
    Text("a built-in token highlighter for the other languages."),
    Text("Scope guide / highlight ([scope] in :config): a line beside the numbers and"),
    Text("a tint that follow the {} () [] pair around the cursor."),
    Blank,
    Heading("CUSTOM LANGUAGES"),
    Text("Put a .toml file in ~/.config/vmacs/languages/. Match files with"),
    Text("`extensions` and/or `filenames` (e.g. [\"Makefile\"]). :reload applies"),
    Text("changes and shows any file that failed to load."),
    Text("On Windows use %APPDATA%\\vmacs\\languages\\."),
];

impl HelpRow {
    fn render(&self) -> AnnotatedString {
        match self {
            Self::Heading(text) => {
                let mut line = AnnotatedString::from(text);
                line.add_annotation(AnnotationType::Keyword, 0, text.len());
                line
            }
            Self::Entry(key, description) => {
                let text = format!("  {key:<18} {description}");
                let mut line = AnnotatedString::from(&text);
                line.add_annotation(AnnotationType::Attribute, 2, key.len().saturating_add(2));
                line
            }
            Self::Text(text) => AnnotatedString::from(&format!("  {text}")),
            Self::Blank => AnnotatedString::default(),
        }
    }
}

#[derive(Default)]
pub struct HelpScreen {
    scroll: usize,
    size: Size,
    needs_redraw: bool,
}

impl HelpScreen {
    fn draw_footer(&self, row: RowIdx, first: usize, last: usize, total: usize) -> Result<(), Error> {
        let width = self.size.width;
        let mut spans: Vec<StatusSpan> = Vec::new();

        let badge = " LINES ";
        let range = format!(" {first}-{last} of {total}");
        let left_end = badge.width().saturating_add(range.width());
        spans.push(StatusSpan {
            col: 0,
            text: badge.to_string(),
            fg: BADGE_FG,
            bg: BADGE_BG,
            bold: true,
        });
        spans.push(StatusSpan {
            col: badge.width(),
            text: range,
            fg: TEXT,
            bg: BAR_BG,
            bold: false,
        });

        let budget = width.saturating_sub(left_end).saturating_sub(2);
        let mut shown: Vec<usize> = Vec::new();
        let mut used = 0usize;
        for index in (0..HINTS.len()).rev() {
            let (key, desc) = HINTS[index];
            let gap = if shown.is_empty() { 0 } else { 2 };
            let hint_width = key.width().saturating_add(1).saturating_add(desc.width());
            let add = hint_width.saturating_add(gap);
            if used.saturating_add(add) > budget {
                break;
            }
            used = used.saturating_add(add);
            shown.push(index);
        }
        shown.reverse();

        let mut col = width.saturating_sub(used.saturating_add(1));
        for (n, &index) in shown.iter().enumerate() {
            let (key, desc) = HINTS[index];
            if n > 0 {
                col = col.saturating_add(2);
            }
            spans.push(StatusSpan { col, text: key.to_string(), fg: KEY, bg: BAR_BG, bold: true });
            col = col.saturating_add(key.width());
            let desc_text = format!(" {desc}");
            let desc_width = desc_text.width();
            spans.push(StatusSpan { col, text: desc_text, fg: DIM, bg: BAR_BG, bold: false });
            col = col.saturating_add(desc_width);
        }

        Terminal::print_status_row(row, width, BAR_BG, TEXT, &spans)
    }
    
    pub fn open(&mut self) {
        self.scroll = 0;
        self.set_needs_redraw(true);
    }

    fn content_rows(&self) -> usize {
        self.size.height.saturating_sub(1)
    }

    fn max_scroll(&self) -> usize {
        HELP.len().saturating_sub(self.content_rows())
    }

    pub fn page_size(&self) -> usize {
        self.content_rows().saturating_sub(1).max(1)
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_add(amount).min(self.max_scroll());
        self.set_needs_redraw(true);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
        self.set_needs_redraw(true);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.set_needs_redraw(true);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.max_scroll();
        self.set_needs_redraw(true);
    }
}

impl UIComponent for HelpScreen {
    fn set_needs_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        if self.size.height == 0 {
            return Ok(());
        }
        let content_rows = self.content_rows();
        for offset in 0..content_rows {
            let row = origin_row.saturating_add(offset);
            match HELP.get(self.scroll.saturating_add(offset)) {
                Some(help_row) => Terminal::print_annotated_row(row, 0, &help_row.render(), None, false)?,
                None => Terminal::print_row(row, "")?,
            }
        }
        let total = HELP.len();
        let first = self.scroll.saturating_add(1).min(total);
        let last = self.scroll.saturating_add(content_rows).min(total);
        self.draw_footer(origin_row.saturating_add(content_rows), first, last, total)
    }
}