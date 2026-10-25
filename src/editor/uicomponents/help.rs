#![allow(
    clippy::indexing_slicing, clippy::arithmetic_side_effects, clippy::integer_division,
    clippy::as_conversions
)]

use std::io::Error;

use crate::prelude::*;

use super::super::{AnnotatedString, AnnotationType, StatusSpan, Terminal};
use super::UIComponent;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use super::super::theme as t;

const BAR_BG: Color = t::SURFACE; const TEXT: Color = t::TEXT; const DIM: Color = t::TEXT_DIM;
const KEY: Color = t::BLUE; const BADGE_FG: Color = t::ON_ACCENT; const BADGE_BG: Color = t::BADGE_NEUTRAL;

// Dropped from the right as the footer narrows, so the last ones matter most.
const HINTS: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("a", "all"),
    ("/", "search"),
    ("Enter", "fold"),
    ("Tab", "page"),
    ("q", "close"),
];

pub enum HelpLine {
    Heading(String),
    Entry(String, String),
    Action(String, String, String),
    Text(String),
    Blank,
}

fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}


enum HelpRow {
    Heading(&'static str),
    Entry(&'static str, &'static str),
    Text(&'static str),
    Blank,
}

use HelpRow::{Blank, Entry, Heading, Text};

const HELP: &[HelpRow] = &[
    Heading("START HERE"),
    Entry("j k  Enter  /", "in this screen: move, fold a category, search"),
    Entry(":", "command prompt (Tab completes; :help opens this screen)"),
    Entry("i  Esc", "type text / back to Control mode"),
    Entry("C-x  C-q", "save / quit"),
    Entry("C-s  /", "search (incremental / vim-style)"),
    Entry("Tab", "switch between Help and Keys & Actions"),
    Entry("q  Esc", "close (Esc first clears a search)"),
    Blank,
    Heading("MODES"),
    Entry("CONTROL", "default mode: vim-style motions and operators"),
    Entry("INPUT", "type text (enter with i a I A o O); Esc goes back"),
    Entry("MARK", "select text (enter with v); v or Esc goes back"),
    Text("Control and Mark mode use a block cursor that stays on a character, as in Vim:"),
    Text("it cannot pass the last character, h / l do not wrap, Esc from Input steps back"),
    Text("one column, and a Mark selection includes the character under the cursor."),
    Text("Moving up or down remembers your column across short lines."),
    Blank,
    Heading("UNIVERSAL MOTIONS (Control, Input and Mark mode)"),
    Entry("C-Left  C-Right", "previous / next word"),
    Entry("A-Up  A-Down", "previous / next paragraph (Control and Mark: { and })"),
    Entry("A-Left  A-Right", "character left / right"),
    Entry("Home", "first non-blank character; press again for column 0"),
    Entry("End", "end of line"),
    Entry("C-Up  C-Down", "scroll the view a line; the cursor moves only if it must"),
    Entry("C-Home  C-End", "start / end of buffer"),
    Entry("PgUp  PgDn", "move by a screenful"),
    Entry("Shift + any of the above", "select (C-S-Left/Right: word, A-S-Left/Right: paragraph)"),
    Text("In Mark mode every motion extends the selection."),
    Blank,
    Heading("CONTROL MODE: MOTIONS"),
    Entry("h  j  k  l", "left, down, up, right"),
    Entry("0  $", "start / end of line"),
    Entry("w  b", "next / previous word"),
    Entry("{  }", "previous / next paragraph"),
    Entry("e", "end of word"),
    Entry("H  M  L", "top / middle / bottom line of the screen"),
    Entry("zt  zz  zb", "scroll so the cursor line is at the top / center / bottom"),
    Entry("^  g_", "first / last non-blank character of the line"),
    Entry("%", "jump to the matching bracket"),
    Entry("f F t T + char", "find a character on the line (t/T stop before it)"),
    Entry("C-d  C-u", "half a screen down / up"),
    Entry("gg  G", "start / end of buffer"),
    Entry("C-o  Tab", "jump list: back / forward (G, gg, %, {, }, H M L, goto, searches)"),
    Entry("Arrows, PgUp, PgDn", "move (Home / End: start / end of line)"),
    Entry("C-Home / End", "start / end of buffer"),
    Entry("zh  zl  zH  zL", "scroll the view left / right (a column / half a screen)"),
    Entry("zs  ze", "cursor column to the left / right edge of the view"),
    Entry("+  -", "first non-blank of the next / previous line (works with d y c)"),
    Entry("Ctrl + wheel", "scroll half a screen per notch"),
    Blank,
    Heading("CONTROL MODE: EDITING"),
    Entry("i  a", "insert before / after the cursor"),
    Entry("I  A", "insert at first non-blank / line end"),
    Entry("o  O", "open a new line below / above"),
    Entry("x", "delete the character under the cursor"),
    Entry("[n] d y c > < gc", "operators: dd dw d$ diw ci\" da( yy cc(clipboard) >> << gcc"),
    Entry("i / a + obj", "text objects: w W p \" ' ` ( ) [ ] { } (iw, a\", ci(, dap)"),
    Entry("[n]x  [n]p  [n]u", "counts work on motions, x, p, u, C-r, dd, yy, G (5G = line 5)"),
    Entry(".", "repeat the last change (including text typed after c)"),
    Entry("J  ~  r<char>", "join lines / toggle case / replace character (rr = redo)"),
    Entry("Y  C  S  s  X", "yank line / change to end / change line / change char / delete before"),
    Entry("W  B  E", "WORD motions (w b e stop at punctuation, these do not)"),
    Entry("za zo zc zR zM", "toggle / open / close fold, open all / close all"),
    Entry("zj  zk", "next fold start / previous fold end"),
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
    Entry("type over selection", "typing or Enter replaces the selected text"),
    Entry("M-w", "yank the selection"),
    Entry("M-c", "copy the selection to the system clipboard"),
    Entry("Ctrl-Shift-v", "paste from the system clipboard"),
    Entry("C-z  C-r", "undo / redo"),
    Text("Emacs-style shortcuts are supported here"),
    Text("Typing in Input mode is one undo step until Esc or a cursor move."),
    Blank,
    Heading("AUTOCOMPLETE (Input mode, [completion] in :config)"),
    Entry("Tab", "accept the selected suggestion, or complete the word before the cursor"),
    Entry("Down Up C-n C-p", "pick a suggestion (S-Tab: previous)"),
    Entry("Esc", "close the popup; Enter always inserts a newline"),
    Text("Suggestions are words from the buffer (nearest first), then the language's"),
    Text("keywords, types and values. Tab with no word before it indents."),
    Blank,
    Heading("MARK MODE"),
    Entry("h  j  k  l", "extend the selection"),
    Entry("0  $", "extend to start / end of line"),
    Entry("w  b", "extend by word"),
    Entry("{  }", "previous / next paragraph"),
    Entry("e  ^  %", "extend to word end / first non-blank / matching bracket"),
    Entry("H  M  L", "extend to the top / middle / bottom of the screen"),
    Entry("f F t T + char", "extend to a character on the line"),
    Entry("C-d  C-u", "extend half a screen down / up"),
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
    Entry("M-g", "go to line or line:column (:goto N, :N, :N:C also work)"),
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
    Entry(":pin [file]", "pin the current file (or a path) to the dashboard"),
    Entry(":unpin [file]", "remove a pin (use the full path for a deleted file)"),
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
    Entry(":jumps", "show the jump list"),
    Entry(":sort [n r u i]", "sort lines (:%sort n, :'<,'>sort u)"),
    Entry(":fold  :foldall", "toggle fold / close every fold (:unfoldall opens them)"),
    Entry(":w!", "save even if the file changed on disk"),
    Entry(":path", "toggle the full file path in the status bar"),
    Entry(":ui [ws|full|compact]", "switch the interface size (toggles with no argument)"),
    Entry(":layout [name]", "dashboard layout: center, left or right (cycles with no argument)"),
    Entry(":ro", "toggle read-only; [RO] in the status bar marks it"),
    Entry(":encoding [name]", "show / set how the file is saved"),
    Entry(":eol [lf|crlf|cr]", "show / set line endings"),
    Entry(":reopen NAME", "re-read the file as utf-8, utf-8-bom, utf-16le/be, latin1, cp1252"),
    Entry(":config  :configdir", "edit config.toml / browse the config folder"),
    Entry(":newlang [name]", "new custom language file (edit it, save, :reload)"),
    Entry("/  ?  n  N", "search forward / backward, next / previous match"),
    Entry("*  #", "search the word under the cursor"),
    Entry(":%s/a/b/g", "substitute (flags g i I n; ranges: 10,20  .,$  '<,'>)"),
    Entry(":42  :noh", "go to line 42 / clear search highlight"),
    Entry(":42:10  :col 10", "go to line 42, column 10 / column 10 of this line"),
    Blank,
    Heading("CUSTOM COMMANDS (type \\ in Control mode)"),
    Text("Define them under [commands] in :config, then :reload. Each name maps"),
    Entry("\\name [args]", "run a custom command (Tab completes names)"),
    Text("to a : command or a list of them. {} is replaced by all typed arguments,"),
    Text("{1} {2} ... by the individual ones (use quotes for args with spaces)."),
    Blank,
    Heading("DASHBOARD"),
    Entry("n/i  e  h  q", "new file / open file / help / quit"),
    Entry("1-9  A-K", "open that pinned file (A-K are pins 10-20, hold Shift)"),
    Entry("R 1-9, R 0", "open that recent file (R0 is the 10th; Shift+1-9 also works)"),
    Entry("Up Down  j k", "select a menu item or file; Enter opens it"),
    Entry("s", "restore the last session"),
    Entry("c  d", "edit config.toml / open the config folder"),
    Entry("g  m", "new language syntax file / keys and custom actions"),
    Entry("Tab  Left Right", "switch between the pinned and recent columns"),
    Entry("Enter  l", "open the selected file"),
    Entry("p", "pin / unpin the selected file"),
    Entry("/", "filter both columns (all words must match the path)"),
    Entry("Alt-p", "pin / unpin while filtering"),
    Entry("Esc", "clear the filter"),
    Text("Shift-digit works on US-style layouts; R then a digit works everywhere."),
    Text("Recent files that were moved or deleted are dropped automatically."),
    Text("Pinned files that are missing are hidden, and return if the file does."),
    Blank,
    Heading("YANK HISTORY (off by default)"),
    Text("Enable under [yanks] in :config, then :reload. In memory only."),
    Text("Records yy, y / M-w and c / M-c; deletes and mouse drags are not kept."),
    Entry(":yanks", "pick an entry: Tab / arrows, Enter pastes (empty = newest)"),
    Entry(":yanks clear", "forget the whole history"),
    Blank,
    Heading("BUFFERS (a tab bar shows when 2+ are open)"),
    Entry("A-t  :tabnew", "new buffer (no file: a dashboard tab; :tabnew path opens it)"),
    Entry("A-.  A-,", "next / previous buffer (:bnext :bprev, wheel over the bar)"),
    Entry("A-q  :bd  :bd!", "close the buffer (! discards edits)"),
    Entry(":b N  :ls", "go to buffer N / list buffers"),
    Entry(":wa  :wqa", "save all buffers / save all and quit"),
    Entry(":session", "restore the last session (dashboard: s)"),
    Text("Click a tab to switch, drag to reorder, click its x or middle-click to close."),
    Text("Quitting with unsaved changes in any buffer needs C-q three times or :q!."),
    Blank,
    Heading("MOUSE"),
    Entry("Click", "move the cursor"),
    Entry("Drag", "select text (also copied to the register)"),
    Entry("Scroll wheel", "scroll the view"),
    Entry("Double / triple", "select word / line (drag to extend)"),
    Entry("Middle click", "paste the primary selection; selected text may also be copied."),
    Entry("Shift + wheel", "scroll sideways (a horizontal wheel works too, i hope)"),
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
    Text("Indent guides ([scope] in :config): dim vertical lines at each indent level;"),
    Text("the guide of the {} () [] block around the cursor lights up. Optional tint"),
    Text("and bracket highlight for that pair."),
    Text("a tint that follow the {} () [] pair around the cursor."),
    Blank,
    Heading("CUSTOM KEYBINDS"),
    Text("Bind keys to built-in actions, : commands or \\ custom commands under"),
    Text("[keys.control] / [keys.input] / [keys.mark] / [keys.all] in :config."),
    Entry(":keys", "your keybinds, custom commands and action names"),
    Entry("Tab", "in this screen: switch to the keybinds page and back"),
    Blank,
    Heading("CUSTOM LANGUAGES"),
    Text("Put a .toml file in ~/.config/vmacs/languages/. Match files with"),
    Text("`extensions` and/or `filenames` (e.g. [\"Makefile\"])."),
    Text(":newlang writes a commented starter file there and opens it."),
    Text("changes and shows any file that failed to load."),
    Text("On Windows use %APPDATA%\\vmacs\\languages\\."),
];

struct Item {
    key: String,
    bind: String,
    desc: String,
    plain: bool, // a text line rather than a key/description entry
}

impl Item {
    fn matches(&self, needle: &str) -> bool {
        self.key.to_lowercase().contains(needle)
            || self.bind.to_lowercase().contains(needle)
            || self.desc.to_lowercase().contains(needle)
    }
}

struct Section {
    title: String,
    items: Vec<Item>,
    open: bool,
}

#[derive(Default)]
struct Page {
    sections: Vec<Section>,
    cursor: usize,
    scroll: usize,
    filter: String,
    built: bool,
}

struct Vis {
    s: usize,
    i: Option<usize>,
}

pub enum HelpAction {
    None,
    Close,
    TogglePage,
}

fn push_item(out: &mut Vec<Section>, item: Item) {
    if out.is_empty() {
        out.push(Section { title: "General".to_string(), items: Vec::new(), open: false });
    }
    if let Some(last) = out.last_mut() {
        last.items.push(item);
    }
}

fn entry(key: &str, desc: &str) -> Item {
    Item { key: clean(key), bind: String::new(), desc: clean(desc), plain: false }
}

fn text_item(text: &str) -> Item {
    Item { key: String::new(), bind: String::new(), desc: clean(text), plain: true }
}

fn open_first(mut sections: Vec<Section>) -> Vec<Section> {
    if let Some(first) = sections.first_mut() {
        first.open = true;
    }
    sections
}

fn static_page() -> Page {
    let mut out: Vec<Section> = Vec::new();
    for row in HELP {
        match row {
            HelpRow::Heading(title) => out.push(Section {
                title: clean(title),
                items: Vec::new(),
                open: false,
            }),
            HelpRow::Entry(key, desc) => push_item(&mut out, entry(key, desc)),
            HelpRow::Text(line) => push_item(&mut out, text_item(line)),
            HelpRow::Blank => {}
        }
    }
    Page { sections: open_first(out), built: true, ..Page::default() }
}

fn keys_page(lines: Vec<HelpLine>) -> Page {
    let mut out: Vec<Section> = Vec::new();
    for line in lines {
        match line {
            HelpLine::Heading(title) => {
                out.push(Section { title: clean(&title), items: Vec::new(), open: false });
            }
            HelpLine::Entry(key, desc) => push_item(&mut out, entry(&key, &desc)),
            HelpLine::Action(name, bind, desc) => push_item(
                &mut out,
                Item { key: clean(&name), bind: clean(&bind), desc: clean(&desc), plain: false },
            ),
            HelpLine::Text(line) => push_item(&mut out, text_item(&line)),
            HelpLine::Blank => {}
        }
    }
    Page { sections: open_first(out), built: true, ..Page::default() }
}

// ---- line building -----------------------------------------------------

fn fit(text: &str, max: usize) -> String {
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

struct Lb {
    text: String,
    notes: Vec<(AnnotationType, usize, usize)>,
    width: usize,
    max: usize,
}

impl Lb {
    fn new(max: usize) -> Self {
        Self { text: String::new(), notes: Vec::new(), width: 0, max }
    }

    fn push(&mut self, s: &str, kind: Option<AnnotationType>) {
        let room = self.max.saturating_sub(self.width);
        if room == 0 || s.is_empty() {
            return;
        }
        let shown = fit(s, room);
        let start = self.text.len();
        self.text.push_str(&shown);
        self.width += shown.width();
        if let Some(kind) = kind {
            self.notes.push((kind, start, self.text.len()));
        }
    }

    fn field(&mut self, s: &str, w: usize, kind: Option<AnnotationType>) {
        let shown = fit(s, w);
        let sw = shown.width();
        self.push(&shown, kind);
        self.push(&" ".repeat(w.saturating_sub(sw)), None);
    }

    fn finish(mut self, selected: bool) -> AnnotatedString {
        self.text.push_str(&" ".repeat(self.max.saturating_sub(self.width)));
        let mut line = AnnotatedString::from(&self.text);
        for (kind, start, end) in self.notes {
            line.add_annotation(kind, start, end);
        }
        if selected {
            line.add_annotation(AnnotationType::Selection, 0, self.text.len());
        }
        line
    }
}

// ---- screen ------------------------------------------------------------

#[derive(Default)]
pub struct HelpScreen {
    pages: [Page; 2],
    active: usize,
    filtering: bool,
    outer: Size,
    size: Size,
    needs_redraw: bool,
}

impl HelpScreen {
    fn page(&self) -> &Page {
        &self.pages[self.active]
    }
    fn page_mut(&mut self) -> &mut Page {
        &mut self.pages[self.active]
    }

    fn ensure_help(&mut self) {
        if !self.pages[0].built {
            self.pages[0] = static_page();
        }
    }

    pub fn open(&mut self) {
        self.pages = [Page::default(), Page::default()];
        self.active = 0;
        self.filtering = false;
        self.ensure_help();
        self.set_needs_redraw(true);
    }

    pub fn open_keys(&mut self, lines: Vec<HelpLine>) {
        self.pages = [Page::default(), keys_page(lines)];
        self.active = 1;
        self.filtering = false;
        self.set_needs_redraw(true);
    }

    pub fn toggle_page(&mut self, lines: Vec<HelpLine>) {
        self.filtering = false;
        if self.active == 0 {
            let old: Vec<(String, bool)> = self.pages[1]
                .sections
                .iter()
                .map(|s| (s.title.clone(), s.open))
                .collect();
            let mut page = keys_page(lines);
            for section in &mut page.sections {
                if let Some((_, open)) = old.iter().find(|(title, _)| *title == section.title) {
                    section.open = *open;
                }
            }
            page.cursor = self.pages[1].cursor;
            page.scroll = self.pages[1].scroll;
            page.filter = std::mem::take(&mut self.pages[1].filter);
            self.pages[1] = page;
            self.active = 1;
        } else {
            self.active = 0;
            self.ensure_help();
        }
        self.clamp();
        self.set_needs_redraw(true);
    }

    fn frame(&self) -> Option<(usize, usize, usize, usize)> {
        let o = self.outer;
        if o.width < 64 || o.height < 14 {
            return None;
        }
        let w = o.width.saturating_sub(4).min(108);
        let h = o.height.saturating_sub(2).min(44);
        Some(((o.width - w) / 2, (o.height - h) / 2, w, h))
    }

    fn content_rows(&self) -> usize {
        self.size.height.saturating_sub(2).max(1)
    }

    fn visible(&self) -> Vec<Vis> {
        let page = self.page();
        let needle = page.filter.to_lowercase();
        let filtering = !needle.is_empty();
        let mut out = Vec::new();
        for (si, section) in page.sections.iter().enumerate() {
            let title_hit = filtering && section.title.to_lowercase().contains(&needle);
            let hits: Vec<usize> = section
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| !filtering || title_hit || item.matches(&needle))
                .map(|(i, _)| i)
                .collect();
            if filtering && hits.is_empty() && !title_hit {
                continue;
            }
            out.push(Vis { s: si, i: None });
            if filtering || section.open {
                out.extend(hits.into_iter().map(|i| Vis { s: si, i: Some(i) }));
            }
        }
        out
    }

    fn clamp(&mut self) {
        let n = self.visible().len();
        let rows = self.content_rows();
        let page = self.page_mut();
        page.cursor = page.cursor.min(n.saturating_sub(1));
        page.scroll = page.scroll.min(n.saturating_sub(rows));
        if page.cursor < page.scroll {
            page.scroll = page.cursor;
        }
        if page.cursor >= page.scroll + rows {
            page.scroll = page.cursor + 1 - rows;
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let n = self.visible().len();
        let amount = delta.unsigned_abs();
        let page = self.page_mut();
        page.cursor = if delta < 0 {
            page.cursor.saturating_sub(amount)
        } else {
            (page.cursor + amount).min(n.saturating_sub(1))
        };
        self.clamp();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.move_cursor(isize::try_from(amount).unwrap_or(1));
        self.set_needs_redraw(true);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.move_cursor(-isize::try_from(amount).unwrap_or(1));
        self.set_needs_redraw(true);
    }

    fn set_open(&mut self, mode: Option<bool>) {
        if !self.page().filter.is_empty() {
            return;
        }
        let vis = self.visible();
        let Some(v) = vis.get(self.page().cursor) else {
            return;
        };
        let s = v.s;
        let section = &mut self.page_mut().sections[s];
        section.open = mode.unwrap_or(!section.open);
        self.focus_heading(s);
    }

    fn toggle_all(&mut self) {
        if !self.page().filter.is_empty() {
            return;
        }
        let vis = self.visible();
        let s = vis.get(self.page().cursor).map_or(0, |v| v.s);
        let any_closed = self.page().sections.iter().any(|sec| !sec.open);
        for section in &mut self.page_mut().sections {
            section.open = any_closed;
        }
        self.focus_heading(s);
    }

    fn focus_heading(&mut self, s: usize) {
        let pos = self
            .visible()
            .iter()
            .position(|v| v.s == s && v.i.is_none())
            .unwrap_or(0);
        self.page_mut().cursor = pos;
    }

    fn reset_view(&mut self) {
        let page = self.page_mut();
        page.cursor = 0;
        page.scroll = 0;
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> HelpAction {
        self.set_needs_redraw(true);
        let page_step = isize::try_from(self.content_rows().saturating_sub(1).max(1)).unwrap_or(1);

        if self.filtering {
            match code {
                KeyCode::Esc => {
                    self.page_mut().filter.clear();
                    self.filtering = false;
                    self.reset_view();
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Backspace => {
                    if self.page_mut().filter.pop().is_none() {
                        self.filtering = false;
                    }
                    self.reset_view();
                }
                KeyCode::Down => self.move_cursor(1),
                KeyCode::Up => self.move_cursor(-1),
                KeyCode::Char(c)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.page_mut().filter.push(c);
                    self.reset_view();
                }
                _ => {}
            }
            self.clamp();
            return HelpAction::None;
        }

        match code {
            KeyCode::Esc if !self.page().filter.is_empty() => {
                self.page_mut().filter.clear();
                self.reset_view();
            }
            KeyCode::Esc | KeyCode::Char('q') => return HelpAction::Close,
            KeyCode::Tab | KeyCode::BackTab => return HelpAction::TogglePage,
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::PageDown | KeyCode::Char(' ' | 'f') => self.move_cursor(page_step),
            KeyCode::PageUp | KeyCode::Char('b') => self.move_cursor(-page_step),
            KeyCode::Home | KeyCode::Char('g') => self.reset_view(),
            KeyCode::End | KeyCode::Char('G') => {
                let n = self.visible().len();
                self.page_mut().cursor = n.saturating_sub(1);
            }
            KeyCode::Enter => self.set_open(None),
            KeyCode::Right | KeyCode::Char('l') => self.set_open(Some(true)),
            KeyCode::Left | KeyCode::Char('h') => self.set_open(Some(false)),
            KeyCode::Char('a') => self.toggle_all(),
            _ => {}
        }
        self.clamp();
        HelpAction::None
    }

    // ---- drawing ----

    fn columns(&self, width: usize) -> (usize, usize) {
        let page = self.page();
        let items = page.sections.iter().flat_map(|s| s.items.iter()).filter(|i| !i.plain);
        let key_w = items.clone().map(|i| i.key.width()).max().unwrap_or(0);
        let bind_w = items.map(|i| i.bind.width()).max().unwrap_or(0);
        let kw = key_w.clamp(12, 28).min(width / 3).max(8);
        let bw = if bind_w > 0 && width >= 72 { bind_w.min(18) + 2 } else { 0 };
        (kw, bw)
    }

    fn render_line(&self, v: &Vis, selected: bool, width: usize, kw: usize, bw: usize) -> AnnotatedString {
        let section = &self.page().sections[v.s];
        let mut lb = Lb::new(width);
        lb.push(if selected { "▌" } else { " " }, selected.then_some(AnnotationType::Function));
        match v.i {
            None => {
                let open = section.open || !self.page().filter.is_empty();
                lb.push(if open { " ▾ " } else { " ▸ " }, Some(AnnotationType::Border));
                lb.push(&section.title, Some(AnnotationType::Keyword));
                let n = section.items.iter().filter(|i| !i.plain).count();
                if n > 0 {
                    lb.push(&format!("  {n}"), Some(AnnotationType::Comment));
                }
            }
            Some(i) => {
                let item = &section.items[i];
                lb.push("    ", None);
                if item.plain {
                    lb.push(&item.desc, None);
                } else {
                    lb.field(&item.key, kw, Some(AnnotationType::Attribute));
                    lb.push("  ", None);
                    if bw > 0 {
                        lb.field(&item.bind, bw, Some(AnnotationType::Number));
                        lb.push(&item.desc, None);
                    } else if item.bind.is_empty() {
                        lb.push(&item.desc, None);
                    } else {
                        lb.push(&format!("[{}] ", item.bind), Some(AnnotationType::Number));
                        lb.push(&item.desc, None);
                    }
                }
            }
        }
        lb.finish(selected)
    }

    fn draw_header(&self, row: RowIdx, width: usize, matches: usize) -> Result<(), Error> {
        let mut spans: Vec<StatusSpan> = Vec::new();
        let mut col = 1;
        for (i, name) in [" Help ", " Keys & Actions "].iter().enumerate() {
            let active = i == self.active;
            spans.push(StatusSpan {
                col,
                text: (*name).to_string(),
                fg: if active { BADGE_FG } else { DIM },
                bg: if active { t::BLUE } else { BAR_BG },
                bold: active,
            });
            col += name.width() + 1;
        }
        let page = self.page();
        if self.filtering || !page.filter.is_empty() {
            let caret = if self.filtering { "▏" } else { "" };
            let text = format!("/{}{caret}", page.filter);
            spans.push(StatusSpan {
                col: col + 2,
                text: fit(&text, width.saturating_sub(col + 16)),
                fg: t::TEXT_BRIGHT,
                bg: BAR_BG,
                bold: true,
            });
            let count = format!("{matches} matches");
            if width > count.width() + 1 {
                spans.push(StatusSpan {
                    col: width - count.width() - 1,
                    text: count,
                    fg: t::AMBER,
                    bg: BAR_BG,
                    bold: false,
                });
            }
        } else {
            let hint = "/ search";
            if width > col + hint.width() + 2 {
                spans.push(StatusSpan {
                    col: width - hint.width() - 1,
                    text: hint.to_string(),
                    fg: DIM,
                    bg: BAR_BG,
                    bold: false,
                });
            }
        }
        Terminal::print_status_row(row, width, BAR_BG, TEXT, &spans)
    }

    fn draw_footer(&self, row: RowIdx, pos: usize, total: usize, section: &str) -> Result<(), Error> {
        let width = self.size.width;
        let mut spans: Vec<StatusSpan> = Vec::new();
        let badge = if self.active == 1 { " KEYBINDS " } else { " HELP " };
        spans.push(StatusSpan {
            col: 0,
            text: badge.to_string(),
            fg: BADGE_FG,
            bg: BADGE_BG,
            bold: true,
        });
        let left = fit(&format!(" {pos}/{total}  {section}"), (width / 2).saturating_sub(badge.width()));
        let left_end = badge.width() + left.width();
        spans.push(StatusSpan { col: badge.width(), text: left, fg: TEXT, bg: BAR_BG, bold: false });

        let budget = width.saturating_sub(left_end).saturating_sub(2);
        let mut shown: Vec<usize> = Vec::new();
        let mut used = 0usize;
        for index in (0..HINTS.len()).rev() {
            let (key, desc) = HINTS[index];
            let gap = if shown.is_empty() { 0 } else { 2 };
            let add = key.width() + 1 + desc.width() + gap;
            if used + add > budget {
                break;
            }
            used += add;
            shown.push(index);
        }
        shown.reverse();
        let mut col = width.saturating_sub(used + 1);
        for (n, &index) in shown.iter().enumerate() {
            let (key, desc) = HINTS[index];
            if n > 0 {
                col += 2;
            }
            spans.push(StatusSpan { col, text: key.to_string(), fg: KEY, bg: BAR_BG, bold: true });
            col += key.width();
            let text = format!(" {desc}");
            let w = text.width();
            spans.push(StatusSpan { col, text, fg: DIM, bg: BAR_BG, bold: false });
            col += w;
        }
        Terminal::print_status_row(row, width, BAR_BG, TEXT, &spans)
    }

    fn draw_inner(&mut self, origin: RowIdx) -> Result<(), Error> {
        let Size { height, width } = self.size;
        if height < 3 || width < 10 {
            return Ok(());
        }
        self.clamp();
        let vis = self.visible();
        let matches = vis.iter().filter(|v| v.i.is_some()).count();
        self.draw_header(origin, width, matches)?;

        let rows = self.content_rows();
        let (kw, bw) = self.columns(width);
        let (cursor, scroll) = (self.page().cursor, self.page().scroll);
        for offset in 0..rows {
            let row = origin + 1 + offset;
            match vis.get(scroll + offset) {
                Some(v) => {
                    let line = self.render_line(v, scroll + offset == cursor, width, kw, bw);
                    Terminal::print_annotated_row(row, 0, &line, None, false)?;
                }
                None if vis.is_empty() && offset == 0 => {
                    Terminal::print_row(row, "  No matches")?;
                }
                None => Terminal::print_row(row, "")?,
            }
        }
        let section = vis
            .get(cursor)
            .map(|v| self.page().sections[v.s].title.clone())
            .unwrap_or_default();
        self.draw_footer(origin + height - 1, cursor.saturating_add(1).min(vis.len()), vis.len(), &section)
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
        self.outer = size;
        self.size = match self.frame() {
            Some((_, _, w, h)) => Size { width: w - 2, height: h - 2 },
            None => size,
        };
        self.clamp();
    }

    fn draw(&mut self, origin_row: RowIdx) -> Result<(), Error> {
        let Some((c, r, w, h)) = self.frame() else {
            return self.draw_inner(origin_row);
        };
        for i in 0..self.outer.height {
            Terminal::print_row(origin_row + i, "")?;
        }
        Terminal::set_origin(c + 1, origin_row + r + 1);
        let result = self.draw_inner(0);
        Terminal::set_origin(0, 0);
        result?;
        let title = if self.active == 1 { "KEYBINDS & ACTIONS" } else { "HELP" };
        Terminal::draw_box(origin_row + r, c, w, h, title, t::BLUE)
    }
}


