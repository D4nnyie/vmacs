use crate::prelude::*;
use crossterm::event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};
use std::{
    env,
    io::Error,
    panic::{set_hook, take_hook},
};
mod annotatedstring;
pub mod annotationtype;
mod command;
mod documentstatus;
mod line;
mod terminal;
mod uicomponents;
mod mode;
pub use annotationtype::AnnotationType;
mod annotation;
use annotation::Annotation;
use annotatedstring::AnnotatedString;
use documentstatus::DocumentStatus;
use line::Line;
use terminal::{StatusSpan, Terminal};
use std::path::PathBuf;
use uicomponents::{
    expand_tilde, new_language_file, classify, BigEdit, Notifier, NotifyLevel, BrowserAction, CommandBar, Dashboard, FileBrowser,
    HelpAction, HelpLine, HelpScreen, RecentKey, Header, StatusBar, TabBar, UIComponent, View,
};
use mode::Mode;
use command::NormalState;
mod clipboard;
mod recent;
mod errors;
mod cmdhistory;
mod yankhistory;
use cmdhistory::{CommandHistory, Kind};
mod ex;
use ex::{LineRange, RangeCtx, SortArgs, Substitute};
mod config;
use config::Config;
mod keymap;
mod pins;
mod session;
mod theme;

use self::command::{
    Command::{self, Edit, Move as MoveCmd, Select, System},
    Edit::InsertNewline,
    Move as MoveType,
    ModeCommand,
    FoldOp,
    System::{CommandPrompt, Dismiss, Quit, Resize, Save, SaveAs, Search},
};


const QUIT_TIMES: u8 = 3;

const UPDATE: &str = "The Workflow Update";

#[derive(Eq, PartialEq, Default)]
enum PromptType {
    Search,
    Save,
    SaveAs,
    Command,  
    Custom,
    GotoLine,
    FindForward,
    FindBackward,
    ReplaceFrom,
    ReplaceTo,
    Yanks,
    #[default]
    None,
}

impl PromptType {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
    fn is_centered(&self) -> bool {
        matches!(self, Self::Command | Self::Custom | Self::GotoLine | Self::Save | Self::SaveAs | Self::Yanks)
    }
}

#[derive(Default)]
pub struct Editor {
    should_quit: bool,
    view: View,
    status_bar: StatusBar,
    command_bar: CommandBar,
    prompt_type: PromptType,
    terminal_size: Size,
    title: String,
    quit_times: u8,
    mode: Mode,
    normal_state: NormalState,
    help: HelpScreen,
    showing_help: bool,
    dashboard: Dashboard,
    showing_dashboard: bool,
    browser: FileBrowser,
    showing_browser: bool,
    dragging: bool,
    drag_edge: Option<(RowIdx, ColIdx)>,
    visual_range: Option<(LineIdx, LineIdx)>,
    replace_from: String,
    query_replace: Option<QueryReplace>,
    config: Config,
    last_click: Option<(Instant, RowIdx, ColIdx)>,
    click_count: u8,
    drag_edge_since: Option<Instant>,
    cmd_history: CommandHistory,
    bufs: Vec<View>,
    dash_flags: Vec<bool>,
    active: usize,
    tab_bar: TabBar,
    tab_drag: bool,
    layout: (usize, usize),
    last_change: Option<(command::Edit, String)>,
    recording: Option<(command::Edit, String)>,
    force_next_write: bool,
    header: Header,
    resize_watch: Option<Instant>,
    notify: Notifier,
}

struct QueryReplace {
    from: String,
    to: String,
    current: Option<Location>,
    count: usize,
}

impl Editor {
    
    pub fn new() -> Result<Self, Error> {
        let current_hook = take_hook();
        set_hook(Box::new(move |panic_info| {
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));
        Terminal::initialize()?;

        let mut editor = Self::default();
        editor.header.refresh_workspace();
        editor.bufs.push(View::default());
        editor.dash_flags.push(false);
        editor.cmd_history = CommandHistory::load();
        let (config, config_error) = Config::load();
        editor.apply_config(config);
        let size = Terminal::size().unwrap_or_default();
        editor.handle_resize_command(size);
        editor.update_message("Press M^h or type :help");
        if let Some(message) = config_error {
            editor.update_message(&message);
        }

        let file_arg = match env::args_os().nth(1).map(std::ffi::OsString::into_string) {
            Some(Ok(name)) if !name.is_empty() => Some(name),
            Some(Err(_)) => {
                editor.update_message("ERR: file name is not valid UTF-8");
                None
            }
            _ => None,
        };
        if let Some(file_name) = file_arg.as_deref() {
            let is_new = !std::path::Path::new(file_name).exists();

            if let Err(err) = editor.view.load(file_name) {
                editor.update_message(&errors::describe(
                    "cannot open",
                    file_name,
                    &err,
                ));
            } else if is_new {
                editor.update_message(&format!("New file: {file_name}"));
            } else {
                recent::record(file_name);
            }
        } else {
            editor.refresh_dashboard();
            editor.showing_dashboard = true;
        }
        editor.refresh_status();
        editor.sync_mode_indicator();
        Ok(editor)
    }

    fn header_rows(&self) -> usize {
        let s = self.terminal_size;
        usize::from(self.config.ui.mode == config::UiMode::Workstation && s.height >= 14 && s.width >= 50)
    }

    fn tab_rows(&self) -> usize {
        if self.bufs.len() <= 1 { return 0; }
        let line = self.config.tabs.active_line
            && match self.config.ui.mode {
                config::UiMode::Full => true,
                config::UiMode::Workstation => self.terminal_size.height >= 20,
                config::UiMode::Compact => false,
            };
        1 + usize::from(line)
    }

    fn effective_compact(&self) -> bool {
        self.config.ui.mode == config::UiMode::Compact
            || (self.config.ui.mode == config::UiMode::Workstation && self.terminal_size.width < 70)
    }

    fn centered_prompt(&self, area: Size) -> bool {
        self.in_prompt()
            && self.prompt_type.is_centered()
            && !self.effective_compact()
            && CommandBar::fits_centered(area)
    }

    pub fn run(&mut self) {
        self.sync_terminal_size();
        loop {
            self.sync_terminal_size();
            if !poll(Duration::ZERO).unwrap_or(false) {
                self.refresh_screen();
            }
            if self.should_quit {
                break;
            }
            while !poll(self.idle_wait()).unwrap_or(true) {
                let resized = self.sync_terminal_size();
                let mut animating = self.view.tick_animation()
                    | self.command_bar.tick_animation()
                    | self.dashboard.tick_animation();
                if self.notify.tick() {
                    self.redraw_main();      // moving toasts leave stale cells behind
                    animating = true;
                }
                animating |= self.status_bar.animating() | self.tab_bar.animating();
                if resized || animating || self.autoscroll_drag() || self.status_bar.needs_redraw() {
                    self.refresh_screen();
                }
            }
            match read() {
                    Ok(mut event) => {
                        if matches!(
                            event,
                            Event::Mouse(MouseEvent { kind: MouseEventKind::Drag(_), .. })
                        ) {
                            while poll(Duration::ZERO).unwrap_or(false) {
                                match read() {
                                    Ok(next @ Event::Mouse(MouseEvent {
                                        kind: MouseEventKind::Drag(_),
                                        ..
                                    })) => event = next,
                                    Ok(other) => {
                                        self.evaluate_event(event);
                                        event = other;
                                        break;
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                        self.evaluate_event(event);
                    }
                    Err(err) => {
                        #[cfg(debug_assertions)]
                        {
                            panic!("Could not read event: {err:?}");
                        }
                        #[cfg(not(debug_assertions))]
                        {
                            let _ = err;
                        }
                    }
                }
            if self.view.take_blocked() {
                self.update_message("E45: 'readonly' option is set (:ro toggles)");
            }
            if let Some(notice) = self.view.take_notice() {
                self.update_message(&notice);
            }
            if let Some(edit) = self.view.take_big_edit() {
                match edit {
                    BigEdit::Deleted(n) => self.notify.push(NotifyLevel::Warn, "Deleted lines", &format!("{n} lines removed · u to undo")),
                    BigEdit::Pasted(n) => self.notify.push(NotifyLevel::Info, "Pasted", &format!("{n} lines inserted")),
                }
            }
            self.refresh_status();
        }
    }

    fn main_size(&self) -> Size {
        Size {
            height: self.terminal_size.height.saturating_sub(self.layout.0.saturating_add(self.layout.1)),
            width: self.terminal_size.width,
        }
    }

    fn view_area(&self) -> Size {
        self.main_size()
    }

    fn refresh_screen(&mut self) {
        if self.terminal_size.height == 0 || self.terminal_size.width == 0 {
            return;
        }
        let desired = self.desired_layout();
        if desired != self.layout {
            self.layout = desired;
            let _ = Terminal::clear_screen();
            self.apply_layout();
        }
        let (top, bottom) = self.layout;
        let _ = Terminal::begin_frame();
        let bottom_row = self.terminal_size.height.saturating_sub(1);
        let area = self.view_area();
        let centered = self.centered_prompt(area);
        let _ = Terminal::hide_caret();

        let hdr = self.header_rows();
        if hdr > 0 {
            let n = self.bufs.len();
            self.header.set_info(if n > 1 { format!("{n} buffers") } else { String::new() });
            self.header.render(0);
        }
        if top > hdr {
            self.sync_tab_bar();
            self.tab_bar.render(hdr);
        }
        if area.height > 0 {
            let main_dirty = if self.showing_help {
                self.help.needs_redraw()
            } else if self.showing_browser {
                self.browser.needs_redraw()
            } else if self.showing_dashboard {
                self.dashboard.needs_redraw()
            } else {
                self.view.needs_redraw()
            };
            if self.showing_help {
                self.help.render(top);
            } else if self.showing_browser {
                self.browser.render(top);
            } else if self.showing_dashboard {
                self.dashboard.render(top);
            } else {
                self.view.render(top);
            }
            if centered && main_dirty {
                self.command_bar.set_needs_redraw(true);
            }
        }
        if !centered {
            let _ = self.notify.render(top, area);
        }
        if centered {
            self.command_bar.render_centered(area);
        }
        if self.in_prompt() && !centered {
            self.command_bar.render(bottom_row);
        } else if bottom > 0 {
            self.status_bar.render(bottom_row);
        }

        let ghost = if self.in_prompt() || self.showing_help || self.showing_browser || self.showing_dashboard {
            None
        } else {
            self.view.ghost_cursor()
        };

        let new_caret_pos = if ghost.is_some() || self.showing_help || self.showing_browser {
            None
        } else if self.in_prompt() {
            if centered {
                self.command_bar.caret_position_centered(area)
            } else {
                Some(Position {
                    row: bottom_row,
                    col: self.command_bar.caret_position_col(),
                })
            }
        } else if self.showing_dashboard {
            None
        } else {
            self.view
                .caret_position()
                .map(|pos| Position { row: pos.row.saturating_add(top), col: pos.col })
        };
        if let Some(pos) = new_caret_pos {
            debug_assert!(pos.col <= self.terminal_size.width);
            debug_assert!(pos.row <= self.terminal_size.height);
            let _ = Terminal::move_caret_to(pos);
            let _ = if self.in_prompt() || self.mode == Mode::Insert {
                Terminal::set_cursor_blinkingbar()
            } else if self.mode == Mode::Visual && self.config.ui.mode == config::UiMode::Workstation {
                Terminal::set_cursor_underline()   // Mark: doesn't cover the selected character
            } else {
                Terminal::set_cursor_block()
            };
            let _ = Terminal::show_caret();
        }
        if let Some(pos) = ghost {
            if pos.col < self.terminal_size.width {
                let mode_name = match self.mode {
                    Mode::Normal => "CONTROL",
                    Mode::Insert => "INPUT",
                    Mode::Visual => "MARK",
                };
                let _ = Terminal::print_styled_at(
                    Position { row: pos.row.saturating_add(top), col: pos.col },
                    " ",
                    theme::ON_ACCENT,
                    theme::mode_color(mode_name),
                    false,
                );
            }
        }
        let _ = Terminal::end_frame();
        let _ = Terminal::execute();
    }

    fn refresh_status(&mut self) {
        let status = self.view.get_status();
        let title = format!("{} - {NAME}", status.file_name);
        self.status_bar.update_status(status);
        if title != self.title && matches!(Terminal::set_title(&title), Ok(())) {
            self.title = title;
        }
    }

    fn evaluate_event(&mut self, event: Event) {
        let should_process = match &event {
            Event::Key(KeyEvent { kind, .. }) => kind == &KeyEventKind::Press,
            Event::Resize(_, _) => true,
            Event::Mouse(_) => true,
            Event::Paste(_) => true,
            _ => false,
        };

        if !should_process {
            return;
        }

        if let Event::Mouse(mouse) = &event {
            if self.handle_tab_mouse(mouse) {
                return;
            }
        }

        if self.query_replace.is_some() {
            self.handle_query_replace_event(event);
            return;
        }

        if let Event::Paste(text) = &event {
            self.handle_paste(text);
            return;
        }

        if self.showing_help {
            self.handle_help_event(event);
            return;
        }

        if self.showing_browser {
            self.handle_browser_event(event);
            return;
        }

        if Self::is_help_shortcut(&event) && !self.in_prompt() {
            self.open_help();
            return;
        }

        if !self.in_prompt() && self.handle_buffer_shortcut(&event) {
            return;
        }


        if self.showing_dashboard && !self.in_prompt() {
            self.handle_dashboard_event(event);
            return;
        }

        if !self.in_prompt() && self.try_custom_key(&event) {
            return;
        }

        if !self.in_prompt() && self.handle_modal_shortcut(&event) {
            return;
        }

        if self.in_prompt() && matches!(self.prompt_type, PromptType::Command | PromptType::Custom | PromptType::Yanks) {
            if let Event::Key(KeyEvent { code: KeyCode::BackTab, .. }) = event {
                self.command_bar.cycle_suggestion(false);
                self.mark_main_dirty();
                return;
            }
        }

        if self.in_prompt() && self.handle_prompt_shortcut(&event) {
            return;
        }
        if !self.in_prompt() && self.handle_emacs_shortcut(&event) {
            return;
        }

        if !self.in_prompt() && self.handle_completion_key(&event) {
            return;
        }

        let command = if self.in_prompt() {
            Command::try_from(event).ok()
        } else {
            match self.mode {
                Mode::Insert => Command::try_from_insert(event).ok(),
                Mode::Normal => Command::try_from_normal(event, &mut self.normal_state).ok().flatten(),
                Mode::Visual => Command::try_from_visual_with(event, &mut self.normal_state).ok().flatten(),
            }
        };

        if let Some(command) = command {
            if self.mode == Mode::Normal && !self.in_prompt() {
                self.process_counted(command);
            } else {
                self.process_command(command);
            }
        }
        self.sync_completion();
    }
    
    fn process_counted(&mut self, command: Command) {
        let count = self.normal_state.take_count();
        let Some(n) = count else {
            self.process_command(command);
            return;
        };
        match command {
            MoveCmd(MoveType::BufferEnd | MoveType::BufferStart) => {
                self.view.seal_undo_group();
                self.view.goto_line(n.saturating_sub(1));
            }
            MoveCmd(motion) => {
                for _ in 1..n {
                    self.view.handle_move_command(motion);
                }
                self.process_command(command);
            }
            Edit(
                edit @ (command::Edit::DeleteCharYank
                | command::Edit::Paste
                | command::Edit::PasteBefore
                | command::Edit::Undo
                | command::Edit::Redo),
            ) => {
                for _ in 1..n {
                    self.view.handle_edit_command(edit);
                }
                self.process_command(command);
            }
            _ => self.process_command(command),
        }
    }

    fn process_command(&mut self, command: Command) {
            if let System(Resize(size)) = command {
                let size = Terminal::size().unwrap_or(size);
                self.handle_resize_command(size);
                return;
            }
            match self.prompt_type {
                PromptType::Search => self.process_command_during_search(command),
                PromptType::Save => self.process_command_during_save(command),
                PromptType::SaveAs => self.process_command_during_save(command),
                PromptType::Command | PromptType::Custom => self.process_command_during_command_prompt(command),
                PromptType::GotoLine => self.process_command_during_goto(command),
                PromptType::Yanks => self.process_command_during_yanks(command),
                PromptType::FindForward | PromptType::FindBackward => {
                    self.process_command_during_find(command)
                }
                PromptType::ReplaceFrom | PromptType::ReplaceTo => {
                    self.process_command_during_replace_prompt(command)
                }
                PromptType::None => self.process_command_no_prompt(command),
            }
    }

    fn process_command_no_prompt(&mut self, command: Command) {
            if matches!(command, System(Quit)) {
                self.handle_quit_command();
                return;
            }
            self.reset_quit_times();

            match command {
                System(Quit | Resize(_) | Dismiss) => {}
                System(Search) => self.set_prompt(PromptType::Search),
                System(Save) => self.handle_save_command(),
                System(SaveAs) => self.handle_save_as_command(),
                System(CommandPrompt) => self.set_prompt(PromptType::Command),
                Edit(edit_command) => {
                    if self.mode == Mode::Insert
                        && matches!(
                            edit_command,
                            command::Edit::Insert(_) | command::Edit::InsertNewline
                        )
                    {
                        self.view.replace_selection_for_typing();
                    }
                    self.run_edit(edit_command);
                    if self.mode == Mode::Visual && Self::is_selection_edit(edit_command) {
                        self.mode = Mode::Normal;
                        self.sync_mode_indicator();
                    }
                }
                MoveCmd(move_command) => {
                    self.view.seal_undo_group();
                    self.view.handle_move_command(move_command);
                }
                Select(move_command) => {
                    self.view.seal_undo_group();
                    self.view.handle_select_command(move_command);
                }
                Command::Mode(mode_command) => self.handle_mode_command(mode_command),
                Command::Mouse(mouse_command) => self.handle_mouse_command(mouse_command),  
            }
            if self.mode != Mode::Insert {
                self.view.seal_undo_group();
            }
            self.view.normalize_cursor();
    }


    fn process_command_during_command_prompt(&mut self, command: Command) {
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
            }
            Edit(InsertNewline) => {
                let custom = self.prompt_type == PromptType::Custom;
                let input = self.command_bar.value();
                self.cmd_history
                    .record(if custom { Kind::Custom } else { Kind::Ex }, &input);
                self.set_prompt(PromptType::None);
                if custom {
                    self.execute_custom_line(&input);
                } else {
                    self.execute_command_line(&input);
                }
            }
            Edit(command::Edit::InsertIndent) => {
                self.command_bar.cycle_suggestion(true);
                self.mark_main_dirty();
            }
            Edit(edit_command) => {
                self.command_bar.handle_edit_command(edit_command);
                self.mark_main_dirty();
            }
            MoveCmd(MoveType::Up) => {
                self.command_bar.history_prev();
                self.mark_main_dirty();
            }
            MoveCmd(MoveType::Down) => {
                self.command_bar.history_next();
                self.mark_main_dirty();
            }
            MoveCmd(MoveType::Right) => {
                self.command_bar.accept_suggestion();
                self.mark_main_dirty();
            }
            System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Select(_) | Command::Mode(_) | Command::Mouse(_) => {}
        }
    }

    fn is_selection_edit(edit: command::Edit) -> bool {
        matches!(
            edit,
            command::Edit::DeleteSelection
                | command::Edit::YankSelection
                | command::Edit::YankSelectionSystem
        )
    }

    fn is_repeatable(edit: command::Edit) -> bool {
        use command::Edit as E;
        match edit {
            E::Operate { op, .. } => op != command::Op::Yank,
            E::JoinLines(_) | E::ToggleCase(_) | E::ReplaceChar(..) | E::DeleteCharYank
            | E::DeleteToLineEnd | E::Paste | E::PasteBefore => true,
            _ => false,
        }
    }

    fn run_edit(&mut self, edit: command::Edit) {
        if matches!(edit, command::Edit::Repeat) {
            self.repeat_change();
            return;
        }
        self.view.handle_edit_command(edit);
        if self.mode == Mode::Insert {
            if let Some((_, text)) = self.recording.as_mut() {
                match edit {
                    command::Edit::Insert(c) => text.push(c),
                    command::Edit::InsertNewline => text.push('\n'),
                    command::Edit::DeleteBackward => {
                        text.pop();
                    }
                    _ => {}
                }
            }
        } else if Self::is_repeatable(edit) {
            if self.view.take_want_insert() {
                self.mode = Mode::Insert;
                self.sync_mode_indicator();
                self.recording = Some((edit, String::new()));
            } else {
                self.last_change = Some((edit, String::new()));
            }
        }
    }

    fn repeat_change(&mut self) {
        let Some((edit, text)) = self.last_change.clone() else {
            self.update_message("Nothing to repeat");
            return;
        };
        self.view.seal_undo_group();
        self.view.handle_edit_command(edit);
        if self.view.take_want_insert() {
            self.view.set_block_cursor(false);
            if !text.is_empty() {
                self.view.paste_text(&text);
            }
            self.view.leave_insert();
            self.view.set_block_cursor(true);
        }
        self.view.seal_undo_group();
    }

    fn execute_command_line(&mut self, input: &str) {
            let trimmed = input.trim();
            if self.try_ex_command(trimmed) {
                return;
            }
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let cmd = parts.next().unwrap_or("");
            let arg = parts.next().map(str::trim).filter(|s| !s.is_empty());

            match cmd {
                "w" | "write" => {
                    self.execute_write(arg);
                }
                "w!" | "write!" => {
                    self.force_next_write = true;
                    self.execute_write(arg);
                    self.force_next_write = false;
                }
                "fold" | "foldtoggle" => self.fold_cmd(FoldOp::Toggle),
                "foldopen" | "foldo" => self.fold_cmd(FoldOp::Open),
                "foldclose" | "foldc" => self.fold_cmd(FoldOp::Close),
                "foldall" | "foldcloseall" => self.fold_cmd(FoldOp::CloseAll),
                "unfoldall" | "foldopenall" => self.fold_cmd(FoldOp::OpenAll),
                "sort" | "sor" => {
                    let text = format!("sort {}", arg.unwrap_or(""));
                    match ex::parse_sort(&text) {
                        Some(Ok(args)) => self.sort_range(0, usize::MAX, args),
                        Some(Err(message)) => self.update_message(&message),
                        None => self.update_message("Usage: :sort [n] [r] [u] [i]"),
                    }
                }
                "q" | "quit" => self.handle_quit_command_vim(),
                "q!" | "quit!" => self.should_quit = true,
                "wq" | "x" => {
                    if self.execute_write(arg) {
                        if self.other_modified() {
                            self.update_message(
                                "Other buffers have unsaved changes (:wqa saves all, :q! discards)",
                            );
                        } else {
                            self.should_quit = true;
                        }
                    }
                }
                "wa" | "wall" => {
                    self.save_all();
                }
                "wqa" | "xa" | "wqall" => {
                    if self.save_all() {
                        self.should_quit = true;
                    }
                }
                "qa" | "qall" => self.handle_quit_command_vim(),
                "qa!" | "qall!" => self.should_quit = true,
                "tabnew" | "tabe" | "tabedit" | "bnew" => self.new_tab_command(arg),
                "bnext" | "bn" | "tabnext" | "tabn" => self.cycle_buffer(true),
                "bprev" | "bprevious" | "bp" | "tabprev" | "tabprevious" | "tabp" => {
                    self.cycle_buffer(false);
                }
                "bdelete" | "bd" | "tabclose" | "tabc" => self.close_buffer(self.active, false),
                "bdelete!" | "bd!" | "tabclose!" => self.close_buffer(self.active, true),
                "buffer" | "b" | "tab" => match arg.and_then(|text| text.parse::<usize>().ok()) {
                    Some(n) if n >= 1 && n <= self.bufs.len() => self.switch_to(n.saturating_sub(1)),
                    Some(n) => self.update_message(&format!("E86: Buffer {n} does not exist")),
                    None => self.update_message("Usage: :b N"),
                },
                "buffers" | "ls" => self.list_buffers(),
                "session" | "restore" => self.restore_session(),
                "autopairs" | "ap" => {
                    let on = self.view.toggle_autopairs();
                    self.update_message(if on { "autopairs on" } else { "autopairs off" });
                }
                "number" | "nu" => {
                    let on = self.view.toggle_line_numbers();
                    self.update_message(if on { "line numbers on" } else { "line numbers off" });
                }
                "u" | "undo" => {
                    if !self.view.try_undo() {
                        self.update_message("Already at oldest change");
                    }
                }
                "red" | "redo" => {
                    if !self.view.try_redo() {
                        self.update_message("Already at newest change");
                    }
                }
                "e" | "edit" => match arg {
                    Some(target) => self.edit_target(target, false),
                    None => self.open_browser(false, None),
                },
                "e!" | "edit!" => match arg {
                    Some(target) => self.edit_target(target, true),
                    None => self.open_browser(true, None),
                },
                "reload" => self.reload_settings(),
                "goto" | "go" => match arg {
                    Some(target) => self.goto_line_input(target),
                    None => self.set_prompt(PromptType::GotoLine),
                },
                "col" | "column" => match arg.and_then(|text| text.parse::<usize>().ok()) {
                    Some(n) => self.view.goto_position(None, Some(n.saturating_sub(1))),
                    None => self.update_message("Usage: :col N"),
                },
                "path" | "fullpath" => {
                    let on = !self.config.statusbar.full_path;
                    self.config.statusbar.full_path = on;
                    self.status_bar.set_full_path(on);
                    self.update_message(if on { "status bar: full path" } else { "status bar: file name" });
                }
                "ui" => {
                    use config::UiMode as M;
                    let next = match arg {
                        Some("workstation" | "ws") => M::Workstation,
                        Some("full") => M::Full,
                        Some("compact") => M::Compact,
                        None => match self.config.ui.mode { M::Workstation => M::Full, M::Full => M::Compact, M::Compact => M::Workstation },
                        Some(_) => { self.update_message("Usage: :ui [workstation|full|compact]"); return; }
                    };
                    self.config.ui.mode = next;
                    self.apply_ui();
                }
                "layout" | "dashlayout" => {
                    use config::DashLayout as L;
                    let next = match arg {
                        Some("center" | "c") => L::Center,
                        Some("left" | "l") => L::Left,
                        Some("right" | "r") => L::Right,
                        None => match self.config.dashboard.layout {
                            L::Center => L::Left,
                            L::Left => L::Right,
                            L::Right => L::Center,
                        },
                        Some(_) => {
                            self.update_message("Usage: :layout [center|left|right]");
                            return;
                        }
                    };
                    self.config.dashboard.layout = next;
                    self.dashboard.set_layout(next);
                    let name = match next { L::Center => "center", L::Left => "left", L::Right => "right" };
                    if self.config.ui.mode == config::UiMode::Workstation {
                        self.update_message(&format!("Dashboard layout: {name}"));
                    } else {
                        self.update_message(&format!("Dashboard layout: {name} (applies in Workstation mode)"));
                    }
                }
                "jumps" | "ju" => {
                    let lines = self.view.jump_lines();
                    if lines.is_empty() {
                        self.update_message("Jump list is empty");
                    } else {
                        let list: Vec<String> =
                            lines.iter().map(|n| n.saturating_add(1).to_string()).collect();
                        self.update_message(&format!("Jumps (C-o back, Tab forward): {}", list.join(" ")));
                    }
                }
                "readonly" | "ro" => {
                    let on = self.view.toggle_read_only();
                    self.update_message(if on { "Buffer is read-only" } else { "Buffer is writable" });
                }
                "encoding" | "enc" | "fenc" | "fileencoding" => match arg {
                    None => {
                        let label = self.view.encoding_label();
                        self.update_message(&format!("Encoding: {label} (:encoding NAME changes how it is saved)"));
                    }
                    Some(name) => match self.view.set_encoding(name) {
                        Ok(label) => self.update_message(&format!("Will save as {label}")),
                        Err(message) => self.update_message(&message),
                    },
                },
                "eol" | "ff" | "fileformat" => match arg {
                    None => {
                        let label = self.view.get_status().line_ending;
                        self.update_message(&format!("Line endings: {label} (:eol lf|crlf|cr changes them)"));
                    }
                    Some(name) => match self.view.set_line_ending(name) {
                        Ok(label) => self.update_message(&format!("Will save with {label} line endings")),
                        Err(message) => self.update_message(&message),
                    },
                },
                "reopen" | "ree" => self.reopen_command(arg, false),
                "reopen!" | "ree!" => self.reopen_command(arg, true),
                "config" => self.edit_config(),
                "configdir" | "confdir" => self.open_config_folder(),
                "newlang" | "newlanguage" => self.new_language(arg),
                "pin" => self.pin_command(arg, true),
                "unpin" => self.pin_command(arg, false),
                "keys" | "keybinds" => self.open_keys_help(),
                "yanks" | "yank-history" => self.yanks_command(arg),
                "qr" | "query-replace" => self.start_query_replace(),
                "noh" | "nohlsearch" => self.view.clear_search_highlight(),
                "dashboard" | "dash" => self.go_dashboard(false),
                "version" | "ver" => self.print_version(),
                "dashboard!" | "dash!" => self.go_dashboard(true),
                "help" | "h" => self.open_help(),
                "" => {}
                other => self.update_message(&format!("E492: Not an editor command: {other}")),
            }
    }

    fn process_command_during_goto(&mut self, command: Command) {
        match command {
            System(Dismiss) => self.set_prompt(PromptType::None),
            Edit(InsertNewline) => {
                let input = self.command_bar.value();
                self.set_prompt(PromptType::None);
                self.goto_line_input(&input);
            }
            Edit(edit_command) => {
                self.command_bar.handle_edit_command(edit_command);
                self.mark_main_dirty();
            }
            _ => {}
        }
    }

    fn fold_cmd(&mut self, op: FoldOp) {
        if !self.view.fold_command(op) {
            self.update_message("No fold here");
        }
    }

    fn sort_range(&mut self, first: LineIdx, last: LineIdx, args: SortArgs) {
        let total = self.view.line_count();
        if total == 0 {
            return;
        }
        let last = last.min(total.saturating_sub(1));
        let kept = self.view.sort_lines(first.min(last), last, args);
        self.update_message(&format!("Sorted {kept} lines"));
    }

    fn reopen_command(&mut self, arg: Option<&str>, force: bool) {
        let Some(name) = arg else {
            self.update_message("Usage: :reopen ENCODING (utf-8, utf-8-bom, utf-16le, utf-16be, latin1, cp1252)");
            return;
        };
        if !force && self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (use :reopen! to discard)");
            return;
        }
        let Some(path) = self.view.file_path() else {
            self.update_message("No file to reopen");
            return;
        };
        match self.view.reload_with_encoding(&path, name) {
            Ok(label) => self.update_message(&format!("Reopened as {label}")),
            Err(message) => self.update_message(&format!("Error: {message}")),
        }
    }

    fn goto_line_input(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some((line, col)) = parse_line_col(trimmed) {
            self.view.goto_position(
                line.map(|n| n.saturating_sub(1)),
                col.map(|n| n.saturating_sub(1)),
            );
            return;
        }
        // goto_line clamps to the last line, so 9999 just goes to the end.
        if let Ok(number) = trimmed.parse::<usize>() {
            self.view.goto_line(number.saturating_sub(1));
            return;
        }
        let ctx = RangeCtx {
            current: self.view.cursor_line(),
            total: self.view.line_count(),
            marks: self.visual_range,
        };
        match ex::parse_range(trimmed) {
            Ok((range, rest)) if rest.trim().is_empty() => match range.resolve(&ctx) {
                Ok((_, last)) => self.view.goto_line(last),
                Err(message) => self.update_message(&message),
            },
            Ok(_) => self.update_message(&format!("E16: Invalid line: {trimmed}")),
            Err(message) => self.update_message(&message),
        }
    }

    fn execute_custom_line(&mut self, input: &str) {
        let trimmed = input.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("");
        let args = parts.next().map_or("", str::trim);
        if name.is_empty() {
            return;
        }
        let Some(custom) = self.config.commands.get(name) else {
            let hint = if self.config.commands.is_empty() {
                "none defined, add a [commands] section via :config"
            } else {
                "Tab lists the defined ones"
            };
            self.update_message(&format!("E492: Not a custom command: {name} ({hint})"));
            return;
        };
        let lines = match custom.expand(args) {
            Ok(lines) => lines,
            Err(message) => {
                self.update_message(&format!("E471: \\{name}: {message}"));
                return;
            }
        };
        for line in lines {
            self.execute_command_line(&line);
            if self.should_quit || self.in_prompt() {
                break;
            }
        }
    }
    
    fn execute_write(&mut self, arg: Option<&str>) -> bool {
        if let Some(file_name) = arg {
            self.save(Some(file_name))
        } else if self.view.is_file_loaded() {
            self.save(None)
        } else {
            self.set_prompt(PromptType::Save);
            false
        }
    }
    

    fn desired_layout(&self) -> (usize, usize) {
        let top = self.header_rows() + self.tab_rows();
        let area = Size {
            height: self.terminal_size.height.saturating_sub(top),
            width: self.terminal_size.width,
        };

        let prompt_on_bottom = self.in_prompt() && !self.centered_prompt(area);
        let hide_bottom = self.showing_dashboard
            && !self.config.dashboard.status_bar
            && !self.showing_help
            && !self.showing_browser
            && !self.status_bar.has_message()
            && !prompt_on_bottom;
        (top, usize::from(!hide_bottom))
    }

    fn apply_layout(&mut self) {
        let size = self.terminal_size;
        let main = self.main_size();
        self.view.resize(main);
        for view in &mut self.bufs {
            view.resize(main);
        }
        self.help.resize(main);
        self.dashboard.resize(main);
        self.browser.resize(main);
        let hdr = self.header_rows();
        self.header.resize(Size { height: hdr, width: size.width });
        self.tab_bar.resize(Size { height: self.tab_rows(), width: size.width });
        let compact = self.effective_compact();
        self.notify.set_enabled(self.config.notifications.enabled && !compact);
        self.view.set_compact(compact);
        for view in &mut self.bufs {
            view.set_compact(compact);
        }
        self.dashboard.set_compact(compact);
        self.tab_bar.set_compact(compact);
        self.status_bar.set_compact(compact);
        self.browser.set_compact(compact);
        let bar_size = Size { height: 1, width: size.width };
        self.status_bar.resize(bar_size);
        self.command_bar.resize(bar_size);
        self.command_bar.set_top(self.layout.0);
    }

    fn handle_resize_command(&mut self, size: Size) {
        self.terminal_size = size;
        self.resize_watch = Some(Instant::now());
        self.layout = self.desired_layout();
        let _ = Terminal::clear_screen();
        self.apply_layout();
        self.redraw_all();
    }

    
    #[allow(clippy::arithmetic_side_effects)]
    fn handle_quit_command(&mut self) {
        if !self.view.get_status().is_modified || self.quit_times + 1 == QUIT_TIMES {
            self.should_quit = true;
        } else if self.view.get_status().is_modified {
            self.update_message(&format!(
                "WARNING! File has unsaved changes. Press C^Q {} more times to quit.",
                QUIT_TIMES - self.quit_times - 1
            ));

            self.quit_times += 1;
        }
    }
    fn handle_quit_command_vim(&mut self) {
        if !self.view.get_status().is_modified || self.quit_times + 1 == QUIT_TIMES {
            self.should_quit = true;
        } else if self.view.get_status().is_modified {
            self.update_message(&format!(
                "WARNING! File has unsaved changes. Run q! to force quit.",
            ));
        }
    }
    fn reset_quit_times(&mut self) {
        if self.quit_times > 0 {
            self.quit_times = 0;
            self.update_message("");
        }
    }

    fn handle_save_command(&mut self) {
        if self.view.is_file_loaded() {
            self.save(None);
        } else {
            self.set_prompt(PromptType::Save);
        }
    }
    fn handle_save_as_command(&mut self) {
        self.set_prompt(PromptType::SaveAs);
    }
    fn handle_mode_command(&mut self, command: ModeCommand) {
        if let ModeCommand::Fold(op) = command {
            if !self.view.fold_command(op) {
                self.update_message("No fold here");
            }
            return;
        }
        if self.mode == Mode::Insert {
            if let Some(done) = self.recording.take() {
                self.last_change = Some(done);
            }
        }
        let target = match command {
            ModeCommand::EnterNormal => Mode::Normal,
            ModeCommand::EnterVisual => Mode::Visual,
            _ => Mode::Insert,
        };
        // Like Vim, leaving Input mode steps the cursor back onto a character.
        if self.mode == Mode::Insert && target != Mode::Insert {
            self.view.leave_insert();
        }
        self.mode = target;
        self.sync_mode_indicator();
        match command {
            ModeCommand::EnterNormal => self.view.clear_selection(),
            ModeCommand::EnterInsert => {}
            ModeCommand::EnterInsertAfter => self.view.step_for_append(),
            ModeCommand::EnterInsertLineStart => self.view.move_to_indent_for_insert(),
            ModeCommand::EnterInsertLineEnd => {
                self.view.handle_move_command(MoveType::EndOfLine);
            }
            ModeCommand::EnterInsertNewlineBelow => {
                self.view.handle_move_command(MoveType::EndOfLine);
                self.view.handle_edit_command(InsertNewline);
            }
            ModeCommand::EnterInsertNewlineAbove => self.view.open_line_above(),
            ModeCommand::EnterVisual => self.view.start_selection(),
            ModeCommand::Fold(_) => {}
        }
    }
    fn sync_mode_indicator(&mut self) {
            self.view.set_block_cursor(self.mode != Mode::Insert);
            self.status_bar.update_mode(match self.mode {
                Mode::Normal => "CONTROL",
                Mode::Insert => "INPUT",
                Mode::Visual => "MARK",
            });
    }
    fn process_command_during_save(&mut self, command: Command) {
            match command {
                System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Select(_) | Command::Mode(_) | Command::Mouse(_) => {}
                System(Dismiss) => {
                    self.set_prompt(PromptType::None);
                    self.update_message("Save aborted.");
                }
                Edit(InsertNewline) => {
                    let file_name = self.command_bar.value();
                    let file_name = file_name.trim();
                    self.set_prompt(PromptType::None);
                    if file_name.is_empty() {
                        self.update_message("Save aborted: no file name.");
                    } else {
                        self.save(Some(file_name));
                    }
                }
                Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
            }
    }
    fn save(&mut self, file_name: Option<&str>) -> bool {
        let force = self.force_next_write;
        let overwrote = force && file_name.is_none() && self.view.disk_changed();
        let result = if let Some(name) = file_name {
            self.view.save_as(name)
        } else {
            self.view.save_forced(force)
        };
        match result {
            Ok(()) => {
                let st = self.view.get_status();
                let body = format!("{} · {} lines", st.file_name, st.total_lines);
                self.say(NotifyLevel::Success, "Saved", "File saved successfully.", &body);
                if overwrote {
                    self.notify.push(NotifyLevel::Warn, "Overwritten", "Changes made outside vMACS were replaced");
                }
                if let Some(path) = self.view.file_path() {
                    recent::record(&path);
                }
                true
            }
            Err(err) => {
                let target = file_name
                    .map(str::to_string)
                    .or_else(|| self.view.file_path())
                    .unwrap_or_else(|| "[No Name]".to_string());
                let msg = errors::describe("save failed, file unchanged", &target, &err);
                if msg.contains("changed on disk") {
                    self.status_bar.set_message(&msg);
                    self.notify.push(NotifyLevel::Warn, "File changed on disk", ":w! overwrites it, :e! reloads it");
                } else {
                    self.say(NotifyLevel::Error, "Save failed", &msg, &msg);
                }
                false
            }
        }
    }

    fn process_command_during_search(&mut self, command: Command) {
            match command {
                System(Dismiss) => {
                    self.set_prompt(PromptType::None);
                    self.view.dismiss_search();
                }
                Edit(InsertNewline) => {
                    self.set_prompt(PromptType::None);
                    self.view.accept_search();
                    self.status_bar.flash(theme::BLUE);
                }
                Edit(edit_command) => {
                    self.command_bar.handle_edit_command(edit_command);
                    let query = self.command_bar.value();
                    self.view.search(&query);
                }
                MoveCmd(MoveType::Right | MoveType::Down) => self.view.search_next(),
                MoveCmd(MoveType::Up | MoveType::Left) => self.view.search_prev(),
                System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Select(_) | Command::Mode(_) | Command::Mouse(_) => {}
            }
    }

    fn update_message(&mut self, new_message: &str) {
        self.status_bar.set_message(new_message);
        if let Some((level, title)) = classify(new_message) {
            self.notify.push(level, title, new_message);
            self.flash_for(level);
        }
    }

    fn flash_for(&mut self, level: NotifyLevel) {
        match level {
            NotifyLevel::Success => self.status_bar.flash(theme::GREEN),
            NotifyLevel::Error => self.status_bar.flash(theme::RED),
            _ => {}
        }
    }

    fn say(&mut self, level: NotifyLevel, title: &str, status: &str, body: &str) {
        self.status_bar.set_message(status);
        self.notify.push(level, title, body);
        self.flash_for(level);
    }

    fn redraw_main(&mut self) {
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
        self.help.set_needs_redraw(true);
        self.browser.set_needs_redraw(true);
    }

    fn announce_open(&mut self) {
        let st = self.view.get_status();
        if st.read_only {
            self.notify.push(NotifyLevel::Warn, "Read-only", &format!("{} can't be written · :ro toggles", st.file_name));
        }
        if st.line_ending.ends_with('*') {
            self.notify.push(NotifyLevel::Warn, "Mixed line endings", "They will be normalized on save");
        }
        if st.encoding != "UTF-8" && st.encoding != "UTF-8 BOM" {
            self.notify.push(NotifyLevel::Info, "Encoding", &format!("Opened as {}", st.encoding));
        }
    }

    fn in_prompt(&self) -> bool {
        !self.prompt_type.is_none()
    }

    fn set_prompt(&mut self, prompt_type: PromptType) {
        match prompt_type {
            PromptType::None => {
                self.command_bar.set_suggestions(false);
                self.status_bar.set_needs_redraw(true);
                self.view.set_needs_redraw(true);
                self.dashboard.set_needs_redraw(true);
            }
            PromptType::Save => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_prompt("File name: ");
                self.command_bar.set_chrome("Save", "Enter to save · Esc to cancel");
            }
            PromptType::SaveAs => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_prompt("File name: ");
                self.command_bar.set_chrome("Save As", "Enter to save · Esc to cancel");
            }
            PromptType::Search => {
                self.command_bar.set_suggestions(false);
                self.view.enter_search();
                self.command_bar
                    .set_prompt("I-search (C-s next, C-r prev, C-g quit): ");
            }
            PromptType::Command => {
                self.command_bar.set_suggestions(true);
                self.command_bar.set_custom_entries(None);
                self.command_bar.set_history(
                    self.cmd_history.entries(Kind::Ex),
                    self.cmd_history.usage(Kind::Ex),
                );
                self.command_bar.set_prompt(":");
                self.command_bar.set_chrome("Command", "Tab: complete · ↑↓: history · Esc: cancel");
            }
            PromptType::Custom => {
                let mut entries: Vec<(String, String)> = self
                    .config
                    .commands
                    .iter()
                    .map(|(name, command)| (name.clone(), command.describe()))
                    .collect();
                entries.sort();
                self.command_bar.set_suggestions(true);
                self.command_bar.set_custom_entries(Some(entries));
                self.command_bar.set_history(
                    self.cmd_history.entries(Kind::Custom),
                    self.cmd_history.usage(Kind::Custom),
                );
                self.command_bar.set_prompt("\\");
                self.command_bar.set_chrome("Custom", "Tab: complete · ↑↓: history · Esc: cancel");
            }
            PromptType::Yanks => {
                let entries: Vec<(String, String)> = self
                    .view
                    .yank_entries()
                    .iter()
                    .enumerate()
                    .map(|(i, text)| (i.saturating_add(1).to_string(), yank_preview(text)))
                    .collect();
                self.command_bar.set_suggestions(true);
                self.command_bar.set_custom_entries(Some(entries));
                self.command_bar.set_history(Vec::new(), std::collections::HashMap::new());
                self.command_bar.set_prompt("Paste #: ");
                self.command_bar
                    .set_chrome("Yank history", "Tab/↑↓: pick · Enter: paste · Esc: cancel");
            }
            PromptType::GotoLine => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_custom_entries(None);
                self.command_bar
                    .set_prompt(&format!("Go to line[:col] (1-{}): ", self.view.line_count()));
                self.command_bar
                    .set_chrome("Go to line", "Enter to jump · 42:10 for line:column · Esc to cancel");
            }
            PromptType::FindForward | PromptType::FindBackward => {
                self.command_bar.set_suggestions(false);
                self.view.enter_search();
                self.command_bar.set_prompt(if prompt_type == PromptType::FindForward {
                    "/"
                } else {
                    "?"
                });
            }
            PromptType::ReplaceFrom => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_prompt("Query replace: ");
            }
            PromptType::ReplaceTo => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_prompt("Replace with: ");
            }
        }
        self.command_bar.clear_value();
        let centered = prompt_type.is_centered();
        self.prompt_type = prompt_type;
        if centered {
            self.command_bar.begin_open();
        }
    }

    fn handle_mouse_command(&mut self, command: command::MouseCommand) {
        let top = self.layout.0;
        match command {
            command::MouseCommand::Press(row, col) => {
                self.drag_edge = None;
                self.drag_edge_since = None;
                self.view.seal_undo_group();
                self.dragging = self.is_within_view(row);
                if self.dragging {
                    let clicks = self.register_click(row, col);
                    if self.view.handle_mouse_press(row.saturating_sub(top), col, clicks) {
                        self.dragging = false;
                    }
                }
            }
            command::MouseCommand::Drag(row, col) => {
                if self.dragging {
                    let rel = row.saturating_sub(top);
                    if self.view.is_drag_edge(rel, col) {
                        if self.drag_edge.is_none() {
                            self.drag_edge_since = Some(Instant::now());
                        }
                        self.drag_edge = Some((rel, col));
                    } else {
                        self.drag_edge = None;
                        self.drag_edge_since = None;
                    }
                    self.view.handle_mouse_drag(rel, col, self.drag_step());
                }
            }
            command::MouseCommand::Release(row, col) => {
                self.drag_edge = None;
                self.drag_edge_since = None;
                if std::mem::take(&mut self.dragging) {
                    if let Some(text) = self.view.handle_mouse_release(row.saturating_sub(top), col) {
                        if self.config.mouse.primary_selection {
                            clipboard::copy_to_primary(&text);
                        }
                        if self.config.mouse.copy_on_select {
                            clipboard::copy_to_system_clipboard(&text);
                        }
                    }
                }
            }
            command::MouseCommand::MiddleClick(row, col) => {
                if self.config.mouse.middle_click_paste && self.is_within_view(row) {
                    let text = clipboard::read_primary()
                        .unwrap_or_else(|| self.view.register_text());
                    if !text.is_empty() {
                        self.view.seal_undo_group();
                        self.view.handle_mouse_press(row.saturating_sub(top), col, 1);
                        self.view.paste_text(&text);
                        if self.mode == Mode::Visual {
                            self.mode = Mode::Normal;
                            self.sync_mode_indicator();
                        }
                    }
                }
            }
            command::MouseCommand::ScrollUp => self.view.scroll_view_up(),
            command::MouseCommand::ScrollDown => self.view.scroll_view_down(),
            command::MouseCommand::ScrollLeft => self.view.scroll_view_left(),
            command::MouseCommand::ScrollRight => self.view.scroll_view_right(),
            command::MouseCommand::ScrollUpFast => self.view.scroll_view_fast(false),
            command::MouseCommand::ScrollDownFast => self.view.scroll_view_fast(true),
        }
    }

    fn idle_wait(&self) -> Duration {
        let watching = self.resize_watch.is_some_and(|at| at.elapsed() < Duration::from_secs(2));
        let animating = self.view.animating()
            || self.command_bar.animating()
            || self.dashboard.animating()
            || self.status_bar.animating()
            || self.tab_bar.animating()
            || self.notify.animating();
        Duration::from_millis(if animating {
            16
        } else if self.notify.active() {
            60            // keeps the countdown bar moving
        } else if watching {
            50
        } else if self.drag_edge.is_some() {
            100
        } else {
            500
        })
    }

    fn autoscroll_drag(&mut self) -> bool {
        let Some((row, col)) = self.drag_edge else {
            return false;
        };
        let step = self.drag_step();
        self.view.handle_mouse_drag(row, col, step);
        true
    }

    fn is_within_view(&self, row: RowIdx) -> bool {
        let top = self.layout.0;
        row >= top && row < top.saturating_add(self.view_area().height)
    }

    fn is_help_shortcut(event: &Event) -> bool {
        matches!(
            event,
            Event::Key(KeyEvent { code: KeyCode::F(1), .. })
                | Event::Key(KeyEvent { code: KeyCode::Char('h'), modifiers: KeyModifiers::ALT | KeyModifiers::META, .. })
        )
    }

    fn open_help(&mut self) {
        self.showing_help = true;
        self.help.open();
        self.status_bar.update_mode("HELP");
    }

    fn close_help(&mut self) {
        self.showing_help = false;
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
        self.sync_mode_indicator();
    }

    fn handle_help_event(&mut self, event: Event) {
        match event {
            Event::Resize(..) => {
                if let Ok(command) = Command::try_from(event) {
                    self.process_command(command);
                }
            }
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                if Self::is_help_shortcut(&event) {
                    self.close_help();
                    return;
                }
                match self.help.handle_key(code, modifiers) {
                    HelpAction::Close => self.close_help(),
                    HelpAction::TogglePage => {
                        let lines = self.keys_help_lines();
                        self.help.toggle_page(lines);
                    }
                    HelpAction::None => {}
                }
            }
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => self.help.scroll_up(3),
                MouseEventKind::ScrollDown => self.help.scroll_down(3),
                _ => {}
            },
            _ => {}
        }
    }
    fn close_dashboard(&mut self) {
        self.showing_dashboard = false;
        self.view.set_needs_redraw(true);
        self.view.start_line_fill();
    }

    fn handle_dashboard_event(&mut self, event: Event) {
        match event {
            Event::Resize(..) => {
                if let Ok(command) = Command::try_from(event) {
                    self.process_command(command);
                }
            }
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                if modifiers == KeyModifiers::CONTROL && code == KeyCode::Char('q') {
                    self.handle_quit_command();
                    return;
                }
                match self.dashboard.handle_key(code, modifiers) {
                    RecentKey::Handled => return,
                    RecentKey::Open(path) => {
                        self.open_recent(&path);
                        return;
                    }
                    RecentKey::TogglePin(path) => {
                        self.toggle_pin(&path);
                        return;
                    }
                    RecentKey::Menu(action) => {
                        self.dashboard_menu(action);
                        return;
                    }
                    RecentKey::Ignored => {}
                }
                if modifiers != KeyModifiers::NONE && modifiers != KeyModifiers::SHIFT {
                    return;
                }
                if let KeyCode::Char(c) = code {
                    self.dashboard_menu(c);
                }
            }
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => self.browser.scroll(-3),
                MouseEventKind::ScrollDown => self.browser.scroll(3),
                _ => {}
            },
            
            _ => {}
        }
    }
    fn open_file(&mut self, file_name: &str) {
        if let Some(idx) = self.find_open(file_name) {
            if idx != self.active {
                self.switch_to(idx);
                return;
            }
        }
        if self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (use :e! to discard)");
            return;
        }
        self.force_open_file(file_name);
    }

    fn open_recent(&mut self, path: &str) {
        if !std::path::Path::new(path).is_file() {
            recent::forget(path);
            self.dashboard.remove_entry(path);
            self.update_message(&format!("File no longer exists: {path}"));
            return;
        }
        self.open_file(path);
    }

    fn force_open_file(&mut self, file_name: &str) {
        let exists = std::path::Path::new(file_name).exists();
        if let Err(err) = self.view.load(file_name) {
            self.update_message(&errors::describe("cannot open", file_name, &err));
            return;
        }
        if self.showing_dashboard {
            self.close_dashboard();
        }
        if exists {
            recent::record(file_name);
        } else {
            self.update_message(&format!("New file: {file_name}"));
        }
        self.announce_open();
    }

    fn sync_terminal_size(&mut self) -> bool {
        match Terminal::size() {
            Ok(size) if size != self.terminal_size => {
                self.handle_resize_command(size);
                true
            }
            _ => false,
        }
    }
    fn handle_paste(&mut self, text: &str) {
        self.view.close_completion();
        if self.showing_help || self.showing_browser || (self.showing_dashboard && !self.in_prompt()) {
            return;
        }
        if self.in_prompt() {
            for c in text.chars().take_while(|c| *c != '\n' && *c != '\r') {
                if !c.is_control() {
                    self.command_bar.handle_edit_command(command::Edit::Insert(c));
                }
            }
            self.mark_main_dirty();
            match self.prompt_type {
                PromptType::Search => {
                    let query = self.command_bar.value();
                    self.view.search(&query);
                }
                PromptType::FindForward | PromptType::FindBackward => {
                    let query = self.command_bar.value();
                    let forward = self.prompt_type == PromptType::FindForward;
                    self.view.vim_search_update(&query, forward);
                }
                _ => {}
            }
            return;
        }
        self.reset_quit_times();
        self.view.paste_text(text);
        if self.mode == Mode::Visual {
            self.mode = Mode::Normal;
            self.sync_mode_indicator();
        }
    }
    fn mark_main_dirty(&mut self) {
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
    }

    fn edit_target(&mut self, target: &str, force: bool) {
        let path = expand_tilde(target);
        if path.is_dir() {
            self.open_browser(force, Some(path));
        } else if let Some(name) = path.to_str() {
            if force {
                self.force_open_file(name);
            } else {
                self.open_file(name);
            }
        } else {
            self.update_message("ERR: path is not valid UTF-8");
        }
    }

    fn open_browser(&mut self, force: bool, start: Option<PathBuf>) {
        if !force && self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (use :e! to discard)");
            return;
        }
        let start = start
            .or_else(|| self.view.file_path().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        self.browser.open(&start);
        self.showing_browser = true;
        self.status_bar.update_mode("BROWSE");
    }

    fn close_browser(&mut self) {
        self.showing_browser = false;
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
        self.sync_mode_indicator();
    }

    fn handle_browser_event(&mut self, event: Event) {
        match event {
            Event::Resize(..) => {
                if let Ok(command) = Command::try_from(event) {
                    self.process_command(command);
                }
            }
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                match self.browser.handle_key(code, modifiers) {
                    BrowserAction::None => {}
                    BrowserAction::Close => self.close_browser(),
                    BrowserAction::Open(path) => {
                        self.close_browser();
                        match path.to_str() {
                            Some(name) => self.force_open_file(name),
                            None => self.update_message("ERR: path is not valid UTF-8"),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn go_dashboard(&mut self, force: bool) {
        if !force && self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (use :dash! to discard)");
            return;
        }
        self.view.clear();
        self.mode = Mode::Normal;
        self.normal_state = NormalState::default();
        self.dashboard.clear_filter();
        self.refresh_dashboard();
        self.showing_dashboard = true;
        self.dashboard.begin_open();
        self.sync_mode_indicator();
        self.update_message("");
    }

    fn print_version(&mut self){
        self.update_message(&format!("vMACS {VERSION} -- {UPDATE}"));
    }

    fn process_command_during_find(&mut self, command: Command) {
        let forward = self.prompt_type == PromptType::FindForward;
        match command {
            System(Dismiss) => {
                self.set_prompt(PromptType::None);
                self.view.dismiss_search();
            }
            Edit(InsertNewline) => {
                let query = self.command_bar.value();
                self.set_prompt(PromptType::None);
                match self.view.vim_search_accept(&query, forward) {
                    Ok(()) => self.status_bar.flash(theme::BLUE),
                    Err(message) => self.update_message(&message),
                }
            }
            Edit(edit_command) => {
                // Backspace on an empty prompt cancels, like vim.
                if matches!(edit_command, command::Edit::DeleteBackward)
                    && self.command_bar.value().is_empty()
                {
                    self.set_prompt(PromptType::None);
                    self.view.dismiss_search();
                    return;
                }
                self.command_bar.handle_edit_command(edit_command);
                let query = self.command_bar.value();
                self.view.vim_search_update(&query, forward);
            }
            System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt)
            | MoveCmd(_)
            | Select(_)
            | Command::Mode(_)
            | Command::Mouse(_) => {}
        }
    }

    fn handle_modal_shortcut(&mut self, event: &Event) -> bool {
        let Event::Key(KeyEvent { code: KeyCode::Char(c), modifiers, .. }) = event else {
            return false;
        };
        if !matches!(*modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
            return false;
        }
        match (self.mode, *c) {
            (Mode::Visual, ':') if !self.normal_state.is_pending() => self.open_visual_command(),
            (Mode::Normal, '/' | '?' | 'n' | 'N' | '*' | '#')
                if !self.normal_state.is_pending() =>
            {
                self.view.seal_undo_group();
                let result = match *c {
                    '/' => {
                        self.set_prompt(PromptType::FindForward);
                        Ok(())
                    }
                    '?' => {
                        self.set_prompt(PromptType::FindBackward);
                        Ok(())
                    }
                    'n' => self.view.search_repeat(false),
                    'N' => self.view.search_repeat(true),
                    '*' => self.view.search_word_under_cursor(true),
                    _ => self.view.search_word_under_cursor(false),
                };
                if let Err(message) = result {
                    self.update_message(&message);
                }
            }
            (Mode::Normal, '\\') if !self.normal_state.is_pending() => {
                self.view.seal_undo_group();
                self.set_prompt(PromptType::Custom);
            }
            _ => return false,
        }
        true
    }

    fn open_visual_command(&mut self) {
        self.visual_range = self.view.selection_lines();
        self.view.clear_selection();
        self.mode = Mode::Normal;
        self.sync_mode_indicator();
        self.set_prompt(PromptType::Command);
        if self.visual_range.is_some() {
            for character in "'<,'>".chars() {
                self.command_bar
                    .handle_edit_command(command::Edit::Insert(character));
            }
        }
    }

    fn try_ex_command(&mut self, line: &str) -> bool {
        if let Some((line, col)) = parse_line_col(line) {
            self.view.goto_position(
                line.map(|n| n.saturating_sub(1)),
                col.map(|n| n.saturating_sub(1)),
            );
            return true;
        }
        let ranged = line.chars().next().is_some_and(|c| {
            c.is_ascii_digit() || matches!(c, '%' | '.' | '$' | '\'' | ',' | '+' | '-')
        });
        if !ranged && !line.starts_with('s') {
            return false;
        }
        let ctx = RangeCtx {
            current: self.view.cursor_line(),
            total: self.view.line_count(),
            marks: self.visual_range,
        };
        let (range, rest) = if ranged {
            match ex::parse_range(line) {
                Ok(parsed) => parsed,
                Err(message) => {
                    self.update_message(&message);
                    return true;
                }
            }
        } else {
            (LineRange::Current, line)
        };
        let rest = rest.trim_start();
        if rest.is_empty() {
            if !ranged {
                return false;
            }
            match range.resolve(&ctx) {
                Ok((_, last)) => self.view.goto_line(last),
                Err(message) => self.update_message(&message),
            }
            return true;
        }
        if let Some(parsed) = ex::parse_sort(rest) {
            let range = if ranged { range } else { LineRange::All };
            match (parsed, range.resolve(&ctx)) {
                (Ok(args), Ok((first, last))) => self.sort_range(first, last, args),
                (Err(message), _) | (_, Err(message)) => self.update_message(&message),
            }
            return true;
        }
        match ex::parse_substitute(rest) {
            Ok(Some(sub)) => self.run_substitute(range, &ctx, sub),
            Ok(None) => {
                if !ranged {
                    return false;
                }
                self.update_message(&format!("E492: Not an editor command: {line}"));
            }
            Err(message) => self.update_message(&message),
        }
        true
    }

    fn run_substitute(&mut self, range: LineRange, ctx: &RangeCtx, mut sub: Substitute) {
        if self.view.is_read_only() && !sub.count_only {   // begin_query_replace: drop the `&& ...` part
            self.update_message("E45: 'readonly' option is set (:ro toggles)");
            return;
        }
        let (first, last) = match range.resolve(ctx) {
            Ok(span) => span,
            Err(message) => {
                self.update_message(&message);
                return;
            }
        };
        if sub.pattern.is_empty() {
            match self.view.last_search() {
                Some(pattern) => sub.pattern = pattern.to_string(),
                None => {
                    self.update_message("E35: No previous regular expression");
                    return;
                }
            }
        }
        let (count, lines) = self.view.substitute(first, last, &sub);
        self.view.set_search_pattern(&sub.pattern);
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let message = if count == 0 {
            format!("E486: Pattern not found: {}", sub.pattern)
        } else if sub.count_only {
            format!("{count} match{} on {lines} line{}", if count == 1 { "" } else { "es" }, plural(lines))
        } else {
            format!("{count} substitution{} on {lines} line{}", plural(count), plural(lines))
        };
        self.update_message(&message);
        if count > 0 { self.status_bar.flash(theme::BLUE); }
    }

    fn handle_prompt_shortcut(&mut self, event: &Event) -> bool {
        let Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            ..
        }) = event
        else {
            return false;
        };
        let searching = self.prompt_type == PromptType::Search;
        match *c {
            'g' => self.cancel_prompt(),
            's' if searching => {
                if !self.command_bar.value().is_empty() {
                    self.view.search_next();
                }
            }
            'r' if searching => {
                if !self.command_bar.value().is_empty() {
                    self.view.search_prev();
                }
            }
            _ => return false,
        }
        true
    }

    fn cancel_prompt(&mut self) {
        if matches!(
            self.prompt_type,
            PromptType::Search | PromptType::FindForward | PromptType::FindBackward
        ) {
            self.view.dismiss_search();
        }
        self.set_prompt(PromptType::None);
        self.update_message("Quit");
    }

    fn handle_emacs_shortcut(&mut self, event: &Event) -> bool {
        let Event::Key(KeyEvent { code: KeyCode::Char(c), modifiers, .. }) = event else {
            return false;
        };
        let alt = modifiers.contains(KeyModifiers::ALT) || modifiers.contains(KeyModifiers::META);
        if !alt || modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        match *c {
            '%' => self.start_query_replace(),
            'x' => {
                self.view.seal_undo_group();
                self.set_prompt(PromptType::Command);
            }
            'g' => {
                if self.mode == Mode::Visual {
                    self.view.clear_selection();
                    self.mode = Mode::Normal;
                    self.sync_mode_indicator();
                }
                self.view.seal_undo_group();
                self.set_prompt(PromptType::GotoLine);
            }
            _ => return false,
        }
        true
    }

    fn start_query_replace(&mut self) {
        if self.mode == Mode::Visual {
            self.view.clear_selection();
            self.mode = Mode::Normal;
            self.sync_mode_indicator();
        }
        self.set_prompt(PromptType::ReplaceFrom);
    }

    fn process_command_during_replace_prompt(&mut self, command: Command) {
        match command {
            System(Dismiss) => self.cancel_prompt(),
            Edit(InsertNewline) => {
                let value = self.command_bar.value();
                if self.prompt_type == PromptType::ReplaceFrom {
                    if value.is_empty() {
                        self.set_prompt(PromptType::None);
                        return;
                    }
                    self.replace_from = value;
                    self.set_prompt(PromptType::ReplaceTo);
                } else {
                    self.set_prompt(PromptType::None);
                    self.begin_query_replace(value);
                }
            }
            Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
            _ => {}
        }
    }

    fn begin_query_replace(&mut self, to: String) {
        let from = std::mem::take(&mut self.replace_from);
        self.view.seal_undo_group();
        let origin = self.view.cursor_location();
        self.query_replace = Some(QueryReplace { from, to, current: None, count: 0 });
        self.advance_query_replace(origin);
    }

    fn advance_query_replace(&mut self, origin: Location) {
        let Some(state) = self.query_replace.as_mut() else {
            return;
        };
        match self.view.qr_next(&state.from, origin) {
            Some(start) => {
                state.current = Some(start);
                self.status_bar.update_mode("REPLACE");
                self.update_message("Replace? y: yes  n: skip  !: all  .: this and quit  q: quit");
            }
            None => self.finish_query_replace(),
        }
    }

    fn finish_query_replace(&mut self) {
        let count = self.query_replace.take().map_or(0, |state| state.count);
        self.view.qr_finish();
        self.sync_mode_indicator();
        self.update_message(&format!(
            "Replaced {count} occurrence{}",
            if count == 1 { "" } else { "s" }
        ));
    }

    fn handle_query_replace_event(&mut self, event: Event) {
        match event {
            Event::Resize(..) => {
                if let Ok(command) = Command::try_from(event) {
                    self.process_command(command);
                }
            }
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                let Some((from, to, current)) = self
                    .query_replace
                    .as_ref()
                    .map(|state| (state.from.clone(), state.to.clone(), state.current))
                else {
                    return;
                };
                let Some(start) = current else {
                    self.finish_query_replace();
                    return;
                };
                if modifiers == KeyModifiers::CONTROL {
                    if code == KeyCode::Char('g') {
                        self.finish_query_replace();
                    }
                    return;
                }
                let mut replaced = 0usize;
                let mut next = None;
                let mut done = false;
                match code {
                    KeyCode::Char('y' | ' ') => {
                        next = Some(self.view.qr_replace(&from, &to, start));
                        replaced = 1;
                    }
                    KeyCode::Char('n') | KeyCode::Delete | KeyCode::Backspace => {
                        next = Some(self.view.qr_skip(&from, start));
                    }
                    KeyCode::Char('.') => {
                        self.view.qr_replace(&from, &to, start);
                        replaced = 1;
                        done = true;
                    }
                    KeyCode::Char('!') => {
                        let mut at = start;
                        loop {
                            let after = self.view.qr_replace(&from, &to, at);
                            replaced = replaced.saturating_add(1);
                            match self.view.qr_next(&from, after) {
                                Some(found) => at = found,
                                None => break,
                            }
                        }
                        done = true;
                    }
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => done = true,
                    _ => return,
                }
                if let Some(state) = self.query_replace.as_mut() {
                    state.count = state.count.saturating_add(replaced);
                }
                if done {
                    self.finish_query_replace();
                } else if let Some(origin) = next {
                    self.advance_query_replace(origin);
                }
            }
            _ => {}
        }
    }

    fn reload_settings(&mut self) {
        let (config, config_error) = Config::load();
        self.apply_config(config);
        let (count, errors) = self.view.reload_languages();
        let message = if let Some(error) = config_error {
            error
        } else if let Some(first) = errors.first() {
            let more = errors.len().saturating_sub(1);
            if more > 0 {
                format!("ERR: language {first} (+{more} more)")
            } else {
                format!("ERR: language {first}")
            }
        } else {
            format!(
                "Reloaded config and {count} custom language{}",
                if count == 1 { "" } else { "s" }
            )
        };
        self.update_message(&message);
    }

    #[allow(clippy::integer_division, clippy::arithmetic_side_effects)]
    fn drag_step(&self) -> usize {
        let held = self
            .drag_edge_since
            .map_or(0, |since| since.elapsed().as_millis());
        let step = usize::try_from(held / 250 + 1).unwrap_or(usize::MAX);
        step.min(self.config.mouse.drag_scroll_max.max(1))
    }

    fn register_click(&mut self, row: RowIdx, col: ColIdx) -> u8 {
        let now = Instant::now();
        let window = Duration::from_millis(self.config.mouse.multi_click_ms);
        let repeated = self.last_click.is_some_and(|(at, last_row, last_col)| {
            now.duration_since(at) <= window && last_row == row && last_col.abs_diff(col) <= 2
        });
        self.click_count = if repeated && self.click_count < 3 {
            self.click_count.saturating_add(1)
        } else {
            1
        };
        self.last_click = Some((now, row, col));
        self.click_count
    }

    fn apply_config(&mut self, config: Config) {
        clipboard::set_osc52(config.clipboard.osc52);
        self.status_bar.set_clock_12h(config.statusbar.clock_12h);
        self.view.set_scope(config.scope.guide, config.scope.highlight, config.scope.brackets);
        self.view.configure_yanks(
            config.yanks.enabled,
            config.yanks.max_entries,
            config.yanks.max_entry_bytes,
        );
        self.tab_bar.set_icons(config.tabs.icons);
        self.apply_theme(&config.theme);
        self.dashboard.set_icons(config.dashboard.icons);
        self.config = config;
        let mut views = std::mem::take(&mut self.bufs);
        for view in &mut views {
            self.configure_view(view);
        }
        self.apply_ui();
        self.bufs = views;
    }

    fn yanks_command(&mut self, arg: Option<&str>) {
        if !self.view.yank_history_enabled() {
            self.update_message("Yank history is off: set enabled = true under [yanks] in :config");
            return;
        }
        match arg {
            None if self.view.yank_count() == 0 => self.update_message("Yank history is empty"),
            None => self.set_prompt(PromptType::Yanks),
            Some("clear") => {
                let count = self.view.clear_yanks();
                self.update_message(&format!("Cleared {count} yank{}", if count == 1 { "" } else { "s" }));
            }
            Some(_) => self.update_message("Usage: :yanks [clear]"),
        }
    }

    fn process_command_during_yanks(&mut self, command: Command) {
        match command {
            System(Dismiss) => self.set_prompt(PromptType::None),
            Edit(InsertNewline) => {
                let input = self.command_bar.value();
                self.set_prompt(PromptType::None);
                self.paste_yank(&input);
            }
            Edit(command::Edit::InsertIndent) | MoveCmd(MoveType::Down) => {
                self.command_bar.cycle_suggestion(true);
                self.mark_main_dirty();
            }
            MoveCmd(MoveType::Up) => {
                self.command_bar.cycle_suggestion(false);
                self.mark_main_dirty();
            }
            Edit(edit_command) => {
                self.command_bar.handle_edit_command(edit_command);
                self.mark_main_dirty();
            }
            _ => {}
        }
    }

    fn paste_yank(&mut self, input: &str) {
        let input = input.trim();
        let number = if input.is_empty() {
            1
        } else {
            match input.parse::<usize>() {
                Ok(n) if n >= 1 => n,
                _ => {
                    self.update_message(&format!("E474: Invalid yank number: {input}"));
                    return;
                }
            }
        };
        match self.view.yank_entry(number.saturating_sub(1)) {
            Some(text) => {
                self.view.paste_text(&text);
                if self.mode == Mode::Visual {
                    self.mode = Mode::Normal;
                    self.sync_mode_indicator();
                }
            }
            None => self.update_message(&format!("No yank #{number}")),
        }
    }

    fn try_custom_key(&mut self, event: &Event) -> bool {
        let Event::Key(key) = event else {
            return false;
        };

        if self.normal_state.is_pending() {
            return false;
        }
        let Some(steps) = self.config.keymap.lookup(self.mode, key) else {
            return false;
        };
        let steps = steps.to_vec();
        self.run_key_steps(&steps);
        true
    }

    fn run_key_steps(&mut self, steps: &[String]) {
        for step in steps {
            let step = step.trim();
            if let Some(line) = step.strip_prefix(':') {
                self.view.seal_undo_group();
                self.execute_command_line(line);
            } else if let Some(line) = step.strip_prefix('\\') {
                self.view.seal_undo_group();
                self.execute_custom_line(line);
            } else if let Some(mut cmd) = command::find_action(step) {
                // In Mark mode movement should extend the selection, not drop it.
                if self.mode == Mode::Visual {
                    if let Command::Move(motion) = cmd {
                        cmd = Command::Select(motion);
                    }
                }
                self.process_command(cmd);
            } else {
                self.update_message(&format!("E492: Unknown key action: {step}"));
                break;
            }
            if self.should_quit
                || self.in_prompt()
                || self.showing_help
                || self.showing_browser
                || self.showing_dashboard
            {
                break;
            }
        }
    }

    fn open_keys_help(&mut self) {
        let lines = self.keys_help_lines();
        self.showing_help = true;
        self.help.open_keys(lines);
        self.status_bar.update_mode("HELP");
    }

    fn keys_help_lines(&self) -> Vec<HelpLine> {
        use HelpLine::{Blank, Entry, Heading, Action, Text};
        let mut lines = vec![
            Heading("vMACS - your keybinds and commands".to_string()),
            Text("Scroll: j/k, arrows, PgUp/PgDn, g/G.   Tab: main help.   Close: q or Esc.".to_string()),
            Blank,
        ];

        let binds = self.config.keymap.listing();
        if binds.is_empty() {
            lines.push(Heading("CUSTOM KEYBINDS".to_string()));
            lines.push(Text("None yet. Add a [keys.control], [keys.input], [keys.mark] or".to_string()));
            lines.push(Text("[keys.all] section in :config, then :reload. Syntax below.".to_string()));
        } else {
            let mut current = None;
            for bind in binds {
                if current != Some(bind.section) {
                    if current.is_some() {
                        lines.push(Blank);
                    }
                    lines.push(Heading(format!("KEYBINDS: {}", bind.section.title())));
                    current = Some(bind.section);
                }
                lines.push(Entry(bind.key.clone(), bind.action.clone()));
            }
        }
        lines.push(Blank);

        lines.push(Heading("CUSTOM COMMANDS (type \\ in Control mode)".to_string()));
        let mut commands: Vec<_> = self.config.commands.iter().collect();
        commands.sort_by(|a, b| a.0.cmp(b.0));
        if commands.is_empty() {
            lines.push(Text("None yet. Add a [commands] section in :config, then :reload.".to_string()));
        }
        for (name, command) in commands {
            lines.push(Entry(format!("\\{name}"), command.describe()));
        }
        lines.push(Blank);

        lines.push(Heading("HOW TO BIND".to_string()));
        for text in [
            "In :config, under [keys.control] / [keys.input] / [keys.mark] / [keys.all]:",
            "  ctrl-j = \"move-down\"       a built-in action (names below)",
            "  f5 = \":w\"                  an ex command, like typing it after :",
            "  alt-s = '\\save-quit'       a custom command from [commands]",
            "  f6 = [\"undo\", \":noh\"]      several steps, run in order",
            "Keys: one character, or ctrl- / alt- / shift- plus a character or space,",
            "enter, tab, backspace, delete, insert, up, down, left, right, home, end,",
            "pageup, pagedown, f1-f24. Then :reload.",
            "Files: :config edits config.toml, :configdir opens the config folder,",
            ":newlang [name] makes a custom language file (then :reload).",
            "Custom keys beat the built-in ones. Esc, F1 and alt-h are reserved.",
            "Input mode and [keys.all] cannot bind a bare character; use ctrl- or alt-.",
            "In Mark mode movement actions extend the selection.",
            "Not active on the dashboard, in prompts or in the file browser.",
        ] {
            lines.push(Text(text.to_string()));
        }
        lines.push(Blank);

        lines.push(Heading("ACTION NAMES".to_string()));
        for category in command::CATEGORIES {
            lines.push(Heading(format!("ACTIONS: {}", category.to_uppercase())));
            for action in command::ACTIONS.iter().filter(|a| command::category(a.name) == *category) {
                let bound: Vec<&str> = binds
                    .iter()
                    .filter(|b| b.action == action.name)
                    .map(|b| b.key.as_str())
                    .collect();
                lines.push(Action(
                    action.name.to_string(),
                    bound.join(" "),
                    action.desc.to_string(),
                ));
            }
        }
        lines
    }

    fn refresh_dashboard(&mut self) {
        self.dashboard.set_session(session::count());
        self.dashboard
            .set_entries(pins::load_existing(), recent::load_existing());
    }

    fn pin_command(&mut self, arg: Option<&str>, pin: bool) {
        let target = match arg {
            Some(text) => match expand_tilde(text).to_str() {
                Some(path) => path.to_string(),
                None => {
                    self.update_message("ERR: path is not valid UTF-8");
                    return;
                }
            },
            None => match self.view.file_path() {
                Some(path) => path,
                None => {
                    self.update_message("No file to pin: open or save one first");
                    return;
                }
            },
        };
        let name = std::path::Path::new(&target)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&target)
            .to_string();
        let result = if pin { pins::pin(&target) } else { pins::unpin(&target) };
        match (result, pin) {
            (Ok(true), true) => self.update_message(&format!("Pinned {name}")),
            (Ok(false), true) => self.update_message(&format!("{name} is already pinned")),
            (Ok(true), false) => self.update_message(&format!("Unpinned {name}")),
            (Ok(false), false) => self.update_message(&format!("{name} is not pinned")),
            (Err(message), _) => self.update_message(&format!("Error: {message}")),
        }
        self.refresh_dashboard();
    }

    fn toggle_pin(&mut self, path: &str) {
        let result = if pins::is_pinned(path) {
            pins::unpin(path).map(|_| "Unpinned")
        } else {
            pins::pin(path).map(|_| "Pinned")
        };
        match result {
            Ok(verb) => self.update_message(&format!("{verb}: {path}")),
            Err(message) => self.update_message(&format!("Error: {message}")),
        }
        self.refresh_dashboard();
    }


    // ---- buffers ----------------------------------------------------------

    fn view_ref(&self, idx: usize) -> &View {
        if idx == self.active {
            &self.view
        } else {
            self.bufs.get(idx).unwrap_or(&self.view)
        }
    }

    fn dash_flag(&self, idx: usize) -> bool {
        if idx == self.active {
            self.showing_dashboard
        } else {
            self.dash_flags.get(idx).copied().unwrap_or(false)
        }
    }

    fn any_modified(&self) -> bool {
        (0..self.bufs.len()).any(|i| self.view_ref(i).tab_info().1)
    }

    fn other_modified(&self) -> bool {
        (0..self.bufs.len()).any(|i| i != self.active && self.view_ref(i).tab_info().1)
    }

    fn configure_view(&self, view: &mut View) {
        let c = &self.config;
        view.set_scope(c.scope.guide, c.scope.highlight, c.scope.brackets);
        view.set_big_edit(c.notifications.big_edit_lines);
        view.configure_yanks(c.yanks.enabled, c.yanks.max_entries, c.yanks.max_entry_bytes);
        view.set_style(c.ui.mode == config::UiMode::Workstation, c.ui.animations);
        view.set_compact(self.effective_compact());
        view.set_popup_animation(c.completion.animate);
        view.set_block_cursor(self.mode != Mode::Insert);
    }

    fn new_view(&self) -> View {
        let mut view = View::default();
        self.configure_view(&mut view);
        view.resize(self.main_size());
        view
    }

    fn add_buffer(&mut self, view: View, dashboard: bool) -> usize {
        self.bufs.push(view);
        self.dash_flags.push(dashboard);
        self.bufs.len().saturating_sub(1)
    }

    fn sync_tab_bar(&mut self) {
        let tabs: Vec<(String, bool, String)> = (0..self.bufs.len())
            .map(|i| {
                if self.dash_flag(i) {
                    ("Dashboard".to_string(), false, "Dashboard".to_string())
                } else {
                    let view = self.view_ref(i);
                    let (title, dirty) = view.tab_info();
                    (title, dirty, view.language_name())
                }
            })
            .collect();
        self.tab_bar.set_tabs(tabs, self.active);
    }

    fn find_open(&self, name: &str) -> Option<usize> {
        let target = std::fs::canonicalize(name).ok()?;
        (0..self.bufs.len()).find(|&i| {
            self.view_ref(i)
                .file_path()
                .and_then(|path| std::fs::canonicalize(path).ok())
                .is_some_and(|path| path == target)
        })
    }

    fn after_buffer_change(&mut self) {
        self.mode = Mode::Normal;
        self.normal_state = NormalState::default();
        self.visual_range = None;
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
        self.view.settle();
        self.redraw_all();
        self.sync_mode_indicator();
    }

    fn switch_to(&mut self, idx: usize) {
        if idx == self.active || idx >= self.bufs.len() {
            return;
        }
        self.view.seal_undo_group();
        self.view.clear_selection();
        let old = self.active;
        if let Some(flag) = self.dash_flags.get_mut(old) {
            *flag = self.showing_dashboard;
        }
        // Park the live view, then bring the target in.
        std::mem::swap(&mut self.view, &mut self.bufs[old]);
        std::mem::swap(&mut self.view, &mut self.bufs[idx]);
        self.active = idx;
        self.view.inherit_shared(&mut self.bufs[old]);
        self.showing_dashboard = self.dash_flags.get(idx).copied().unwrap_or(false);
        self.after_buffer_change();
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn cycle_buffer(&mut self, forward: bool) {
        let count = self.bufs.len();
        if count < 2 {
            return;
        }
        let target = if forward {
            (self.active + 1) % count
        } else {
            (self.active + count - 1) % count
        };
        self.switch_to(target);
    }

    fn new_tab(&mut self, file: Option<&str>) {
        if let Some(name) = file {
            if let Some(idx) = self.find_open(name) {
                self.switch_to(idx);
                self.announce_open();
                return;
            }
        }
        let mut view = self.new_view();
        if let Some(name) = file {
            if let Err(err) = view.load(name) {
                self.update_message(&errors::describe("cannot open", name, &err));
                return;
            }
        }
        let idx = self.add_buffer(view, file.is_none());
        self.switch_to(idx);
        match file {
            None => {
                self.dashboard.clear_filter();
                self.refresh_dashboard();
            }
            Some(name) if std::path::Path::new(name).exists() => recent::record(name),
            Some(name) => self.update_message(&format!("New file: {name}")),
        }
    }

    fn new_tab_command(&mut self, arg: Option<&str>) {
        match arg {
            None => self.new_tab(None),
            Some(text) => match expand_tilde(text).to_str() {
                Some(path) => {
                    let path = path.to_string();
                    self.new_tab(Some(&path));
                }
                None => self.update_message("ERR: path is not valid UTF-8"),
            },
        }
    }

    fn close_buffer(&mut self, idx: usize, force: bool) {
        if idx >= self.bufs.len() {
            return;
        }
        if self.bufs.len() == 1 {
            if self.showing_dashboard {
                self.update_message("Nothing to close");
            } else {
                self.go_dashboard(force);
            }
            return;
        }
        let (name, dirty) = if self.dash_flag(idx) {
            ("Dashboard".to_string(), false)
        } else {
            self.view_ref(idx).tab_info()
        };
        if dirty && !force {
            self.update_message(&format!(
                "E37: No write since last change in {name} (use :bd! to discard)"
            ));
            return;
        }
        if idx == self.active {
            let mut old = std::mem::take(&mut self.view);
            self.bufs.remove(idx);
            self.dash_flags.remove(idx);
            let next = idx.min(self.bufs.len().saturating_sub(1));
            self.view = std::mem::take(&mut self.bufs[next]);
            self.active = next;
            self.view.inherit_shared(&mut old);
            self.showing_dashboard = self.dash_flags.get(next).copied().unwrap_or(false);
            self.after_buffer_change();
        } else {
            self.bufs.remove(idx);
            self.dash_flags.remove(idx);
            if idx < self.active {
                self.active = self.active.saturating_sub(1);
            }
        }
    }

    fn move_buffer(&mut self, from: usize, to: usize) {
        let count = self.bufs.len();
        if from >= count || to >= count || from == to {
            return;
        }
        if let Some(flag) = self.dash_flags.get_mut(self.active) {
            *flag = self.showing_dashboard;
        }
        let view = self.bufs.remove(from);
        self.bufs.insert(to, view);
        let flag = self.dash_flags.remove(from);
        self.dash_flags.insert(to, flag);
        self.active = if self.active == from {
            to
        } else if from < self.active && to >= self.active {
            self.active.saturating_sub(1)
        } else if from > self.active && to <= self.active {
            self.active.saturating_add(1)
        } else {
            self.active
        };
    }

    fn handle_buffer_shortcut(&mut self, event: &Event) -> bool {
        let Event::Key(KeyEvent { code: KeyCode::Char(c), modifiers, .. }) = event else {
            return false;
        };
        if !matches!(*modifiers, KeyModifiers::ALT | KeyModifiers::META) {
            return false;
        }
        match *c {
            '.' => self.cycle_buffer(true),
            ',' => self.cycle_buffer(false),
            't' => self.new_tab(None),
            'q' => self.close_buffer(self.active, false),
            _ => return false,
        }
        true
    }

    fn handle_tab_mouse(&mut self, mouse: &MouseEvent) -> bool {
        let (lo, hi) = (self.header_rows(), self.layout.0);
        // Prompt/query: hover suppressed and event falls through to that UI.
        // (No tab bar — `hi == lo` — is NOT a reason to drop events: the bar
        // arms simply never match via `on_bar`, and fold-button hover must
        // still work in single-buffer sessions.)
        if self.in_prompt() || self.query_replace.is_some() { self.tab_drag = false; self.tab_bar.set_hover(None); self.view.set_fold_hover(None); return false; }
        self.sync_tab_bar();
        let (row, col) = (usize::from(mouse.row), usize::from(mouse.column));
        let on_bar = (lo..hi).contains(&row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if on_bar => {
                match self.tab_bar.hit(col) {
                    Some(hit) if hit.close => self.close_buffer(hit.idx, false),
                    Some(hit) => {
                        self.switch_to(hit.idx);
                        self.tab_drag = true;
                    }
                    None => {}
                }
                true
            }
            MouseEventKind::Down(MouseButton::Middle) if on_bar => {
                if let Some(hit) = self.tab_bar.hit(col) {
                    self.close_buffer(hit.idx, false);
                }
                true
            }
            MouseEventKind::Drag(MouseButton::Left) if self.tab_drag => {
                self.drag_tab(col);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if self.tab_drag => {
                self.tab_drag = false;
                true
            }
            MouseEventKind::ScrollUp if on_bar => {
                self.cycle_buffer(false);
                true
            }
            MouseEventKind::ScrollDown if on_bar => {
                self.cycle_buffer(true);
                true
            }
            // Pointer move: update hover highlighting.
            //  - on the tab bar: tab body / close cell
            //  - in the text area: the fold button under the pointer
            // Always clears the *other* target so a single hover is active.
            // Consumed here so it never reaches command dispatch.
            MouseEventKind::Moved => {
                if on_bar {
                    self.tab_bar.set_hover(self.tab_bar.hit(col));
                    self.view.set_fold_hover(None);
                } else {
                    self.tab_bar.set_hover(None);
                    let in_view = !self.showing_help
                        && !self.showing_browser
                        && !self.showing_dashboard
                        && self.is_within_view(row);
                    let fold = if in_view {
                        self.view.fold_cell_line(row.saturating_sub(self.layout.0), col)
                    } else {
                        None
                    };
                    self.view.set_fold_hover(fold);
                }
                true
            }
            _ => false,
        }
    }

    /// Move the dragged (active) tab one step when the pointer is past where it
    /// would land after the swap, which keeps it from flipping back and forth.
    fn drag_tab(&mut self, col: usize) {
        let a = self.active;
        let Some((start, end)) = self.tab_bar.range(a) else {
            return;
        };
        if col >= end {
            if let Some((next_start, next_end)) = self.tab_bar.range(a.saturating_add(1)) {
                let next_width = next_end.saturating_sub(next_start);
                if col > start.saturating_add(next_width) {
                    self.move_buffer(a, a.saturating_add(1));
                }
            }
        } else if col < start && a > 0 {
            if let Some((prev_start, _)) = self.tab_bar.range(a.saturating_sub(1)) {
                if col < prev_start.saturating_add(end.saturating_sub(start)) {
                    self.move_buffer(a, a.saturating_sub(1));
                }
            }
        }
    }

    fn save_all(&mut self) -> bool {
        let mut saved = 0usize;
        let mut failed: Option<String> = None;
        for i in 0..self.bufs.len() {
            let view = if i == self.active { &mut self.view } else { &mut self.bufs[i] };
            if !view.tab_info().1 {
                continue;
            }
            if !view.is_file_loaded() {
                failed.get_or_insert_with(|| "an unnamed buffer was not saved".to_string());
                continue;
            }
            let path = view.file_path();
            match view.save() {
                Ok(()) => {
                    saved = saved.saturating_add(1);
                    if let Some(path) = &path {
                        recent::record(path);
                    }
                }
                Err(err) => {
                    failed.get_or_insert_with(|| {
                        errors::describe("save failed", path.as_deref().unwrap_or("[No Name]"), &err)
                    });
                }
            }
        }
        match &failed {
            Some(message) => self.update_message(message),
            None => self.update_message(&format!("Saved {saved} buffer{}", if saved == 1 { "" } else { "s" })),
        }
        failed.is_none()
    }

    fn list_buffers(&mut self) {
        let list: Vec<String> = (0..self.bufs.len())
            .map(|i| {
                let (name, dirty) = if self.dash_flag(i) {
                    ("Dashboard".to_string(), false)
                } else {
                    self.view_ref(i).tab_info()
                };
                format!(
                    "{}:{name}{}{}",
                    i.saturating_add(1),
                    if dirty { "+" } else { "" },
                    if i == self.active { "*" } else { "" }
                )
            })
            .collect();
        self.update_message(&list.join("  "));
    }

    // ---- sessions ---------------------------------------------------------

    fn save_session(&self) {
        let mut files: Vec<(String, usize)> = Vec::new();
        let mut active = 0usize;
        for i in 0..self.bufs.len() {
            let view = self.view_ref(i);
            let Some(path) = view.file_path() else {
                continue;
            };
            let Ok(abs) = std::fs::canonicalize(path) else {
                continue;
            };
            let Some(abs) = abs.to_str().map(str::to_string) else {
                continue;
            };
            if abs.contains(['\t', '\n', '\r']) {
                continue;
            }
            if i == self.active {
                active = files.len();
            }
            files.push((abs, view.cursor_line()));
            if files.len() >= session::MAX_FILES {
                break;
            }
        }
        session::save(&files, active);
    }

    fn restore_session(&mut self) {
        let Some(saved) = session::load() else {
            self.update_message("No previous session to restore");
            return;
        };
        let reuse = self.showing_dashboard.then_some(self.active);
        let mut first_new: Option<usize> = None;
        let mut target: Option<usize> = None;
        let mut failed = 0usize;
        for (n, entry) in saved.files.iter().enumerate() {
            if let Some(idx) = self.find_open(&entry.path) {
                if n == saved.active {
                    target = Some(idx);
                }
                continue;
            }
            let mut view = self.new_view();
            if view.load(&entry.path).is_err() {
                failed = failed.saturating_add(1);
                continue;
            }
            view.goto_line(entry.line);
            let idx = self.add_buffer(view, false);
            if n == saved.active {
                target = Some(idx);
            }
            first_new.get_or_insert(idx);
        }
        let Some(go) = target.or(first_new) else {
            self.update_message("The session's files could not be opened");
            return;
        };
        self.switch_to(go);
        if let Some(old) = reuse {
            self.close_buffer(old, true);
        }
        let count = saved.files.len().saturating_sub(failed);
        self.update_message(&format!("Restored {count} file{}", if count == 1 { "" } else { "s" }));
    }

    fn dashboard_menu(&mut self, action: char) {
        match action {
            'n' => {
                self.close_dashboard();
                self.mode = Mode::Normal;
                self.sync_mode_indicator();
            }
            'i' => {
                self.close_dashboard();
                self.mode = Mode::Insert;
                self.sync_mode_indicator();
            }
            'c' => self.edit_config(),
            'd' => self.open_config_folder(),
            'g' => self.new_language(None),
            'm' => self.open_keys_help(),
            'e' | 'o' => self.open_browser(false, None),
            's' => self.restore_session(),
            ':' => self.set_prompt(PromptType::Command),
            '\\' => self.set_prompt(PromptType::Custom),
            'h' => self.open_help(),
            'q' => {
                if self.any_modified() {
                    self.update_message("WARNING! A buffer has unsaved changes (:q! discards)");
                } else {
                    self.should_quit = true;
                }
            }
            _ => {}
        }
    }

    fn edit_config(&mut self) {
        match config::ensure_file() {
            Ok(path) => match path.to_str() {
                Some(path) => self.open_file(path),
                None => self.update_message("ERR: config path is not valid UTF-8"),
            },
            Err(message) => self.update_message(&message),
        }
    }

    fn open_config_folder(&mut self) {
        match config::ensure_file() {
            Ok(path) => match path.parent() {
                // Not forced: with unsaved edits this refuses instead of discarding them.
                Some(dir) => self.open_browser(false, Some(dir.to_path_buf())),
                None => self.update_message("ERR: no config folder"),
            },
            Err(message) => self.update_message(&message),
        }
    }

    fn new_language(&mut self, name: Option<&str>) {
        if self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (save or use :e! first)");
            return;
        }
        match new_language_file(name) {
            Ok(path) => match path.to_str() {
                Some(path) => {
                    let path = path.to_string();
                    self.open_file(&path);
                    self.update_message("Edit extensions and keywords, save, then :reload");
                }
                None => self.update_message("ERR: path is not valid UTF-8"),
            },
            Err(message) => self.update_message(&format!("Error: {message}")),
        }
    }

    fn redraw_all(&mut self) {
        self.view.set_needs_redraw(true);
        self.dashboard.set_needs_redraw(true);
        self.help.set_needs_redraw(true);
        self.browser.set_needs_redraw(true);
        self.status_bar.set_needs_redraw(true);
        self.header.set_needs_redraw(true);
        self.command_bar.set_needs_redraw(true);
        self.tab_bar.set_needs_redraw(true);
    }

    fn apply_theme(&mut self, theme: &config::ThemeConfig) {
        let hyprland = env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
        let transparent = match theme.mode {
            config::BackgroundMode::Transparent => true,
            config::BackgroundMode::Solid => false,
            config::BackgroundMode::Auto => hyprland,
        };
        if transparent {
            Terminal::set_theme(None, None);
        } else {
            let background = config::parse_hex(&theme.background);
            if background.is_none() && !theme.background.trim().is_empty() {
                self.update_message("ERR: theme.background must look like #161a22");
            }
            Terminal::set_theme(background, config::parse_hex(&theme.foreground));
        }
        let _ = Terminal::clear_screen();
        self.redraw_all();
    }

    fn apply_ui(&mut self) {
        let ws = self.config.ui.mode == config::UiMode::Workstation;
        let anim = self.config.ui.animations && !self.effective_compact();
        let n = self.config.notifications;
        self.notify.configure(n.enabled && !self.effective_compact(), anim, n.max_visible, n.timeout_ms);
        self.status_bar.set_animate(anim);
        self.tab_bar.set_animate(anim);
        self.dashboard.set_animate(anim);
        self.view.set_big_edit(n.big_edit_lines);
        self.command_bar.set_animate(anim);
        self.command_bar.set_animate_box(anim && self.config.ui.prompt_animation);
        self.view.set_popup_animation(self.config.completion.animate);
        self.dashboard.set_workstation(ws);
        self.tab_bar.set_workstation(ws);
        self.status_bar.set_workstation(ws);
        self.dashboard.set_boxed(ws && self.config.dashboard.boxed);
        self.status_bar.set_powerline(self.config.ui.powerline);
        self.status_bar.set_full_path(self.config.statusbar.full_path);
        self.browser.set_boxed(ws);
        self.dashboard.set_layout(self.config.dashboard.layout);
        self.view.set_style(ws, self.config.ui.animations);
        self.layout = self.desired_layout();
        let _ = Terminal::clear_screen();
        self.apply_layout();
        self.redraw_all();
    }

    fn handle_completion_key(&mut self, event: &Event) -> bool {
        if self.mode != Mode::Insert || !self.config.completion.enabled {
            return false;
        }
        let Event::Key(KeyEvent { code, modifiers, .. }) = event else {
            return false;
        };
        let open = self.view.completion_open();
        let cfg = self.config.completion;
        match (*code, *modifiers) {
            (KeyCode::Tab, KeyModifiers::NONE) => {
                if open {
                    self.view.accept_completion();
                    return true;
                }
                self.view.refresh_completion(1, cfg.max_items, true);
                if self.view.completion_open() {
                    if self.view.completion_count() == 1 {
                        self.view.accept_completion();
                    }
                    return true;
                }
                false
            }
            (KeyCode::BackTab, _) if open => {
                self.view.completion_move(false);
                true
            }
            (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('n'), KeyModifiers::CONTROL) if open => {
                self.view.completion_move(true);
                true
            }
            (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('p'), KeyModifiers::CONTROL) if open => {
                self.view.completion_move(false);
                true
            }
            (KeyCode::Esc, KeyModifiers::NONE) if open => {
                self.view.close_completion();
                true
            }
            _ => false,
        }
    }

    fn sync_completion(&mut self) {
        let cfg = self.config.completion;
        if self.mode == Mode::Insert && cfg.enabled && cfg.auto && !self.in_prompt() {
            self.view.refresh_completion(cfg.min_chars, cfg.max_items, false);
        } else {
            self.view.close_completion();
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if self.should_quit {
            self.save_session();
        }
        let _ = Terminal::terminate();
        if self.should_quit {
            let _ = Terminal::print("Good work.\r\n");
        }
    }
}

fn yank_preview(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = flat.chars().take(60).collect();
    if text.trim().contains('\n') {
        out.push('↵');
    }
    out
}

fn parse_line_col(text: &str) -> Option<(Option<usize>, Option<usize>)> {
    let (line, col) = text.trim().split_once(':')?;
    let parse = |part: &str| -> Result<Option<usize>, ()> {
        let part = part.trim();
        if part.is_empty() {
            Ok(None)
        } else {
            part.parse::<usize>().map(Some).map_err(|_| ())
        }
    };
    let (line, col) = (parse(line).ok()?, parse(col).ok()?);
    (line.is_some() || col.is_some()).then_some((line, col))
}

#[cfg(test)]
mod tests {
    use super::parse_line_col;

    #[test]
    fn line_and_column_syntax() {
        assert_eq!(parse_line_col("42:10"), Some((Some(42), Some(10))));
        assert_eq!(parse_line_col("42:"), Some((Some(42), None)));
        assert_eq!(parse_line_col(":10"), Some((None, Some(10))));
        assert_eq!(parse_line_col("42"), None);
        assert_eq!(parse_line_col(":"), None);
        assert_eq!(parse_line_col("s/a:b/"), None);
    }
}
