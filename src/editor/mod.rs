use crate::prelude::*;
use crossterm::event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
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
    expand_tilde, BrowserAction, CommandBar, Dashboard, FileBrowser, HelpScreen, StatusBar,
    UIComponent, View,
};
use mode::Mode;
use command::NormalState;
mod clipboard;
mod recent;
mod errors;
mod cmdhistory;
use cmdhistory::{CommandHistory, Kind};
mod ex;
use ex::{LineRange, RangeCtx, Substitute};
mod config;
use config::Config;

use self::command::{
    Command::{self, Edit, Move as MoveCmd, Select, System},
    Edit::InsertNewline,
    Move as MoveType,
    ModeCommand,
    System::{CommandPrompt, Dismiss, Quit, Resize, Save, SaveAs, Search},
};


const QUIT_TIMES: u8 = 3;

const UPDATE: &str = "The Identity Update";

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
    #[default]
    None,
}

impl PromptType {
    fn is_none(&self) -> bool {
        *self == Self::None
    }
    fn is_centered(&self) -> bool {
        matches!(self, Self::Command | Self::Custom | Self::GotoLine | Self::Save | Self::SaveAs)
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
            editor.dashboard.set_recent(recent::load_existing());
            editor.showing_dashboard = true;
        }
        editor.refresh_status();
        editor.sync_mode_indicator();
        Ok(editor)
    }

    
    pub fn run(&mut self) {
        self.sync_terminal_size();
        loop {
            if !poll(Duration::ZERO).unwrap_or(false) {
                self.refresh_screen();
            }
            if self.should_quit {
                break;
            }
            if self.should_quit {
                break;
            }
            while !poll(self.idle_wait()).unwrap_or(true) {
                if self.autoscroll_drag() || self.sync_terminal_size() || self.status_bar.needs_redraw() {
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
            self.refresh_status();
        }
    }

    fn view_area(&self) -> Size {
        Size {
            height: self.terminal_size.height.saturating_sub(1),
            width: self.terminal_size.width,
        }
    }

    fn refresh_screen(&mut self) {
        if self.terminal_size.height == 0 || self.terminal_size.width == 0 {
            return;
        }
        let _ = Terminal::begin_frame();
        let bottom_row = self.terminal_size.height.saturating_sub(1);
        let area = self.view_area();
        let centered =
            self.in_prompt() && self.prompt_type.is_centered() && CommandBar::fits_centered(area);
        let _ = Terminal::hide_caret();

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
                self.help.render(0);
            } else if self.showing_browser {
                self.browser.render(0);
            } else if self.showing_dashboard {
                self.dashboard.render(0);
            } else {
                self.view.render(0);
            }
            if centered && main_dirty {
                self.command_bar.set_needs_redraw(true);
            }
        }
        if centered {
            self.command_bar.render_centered(area);
        }
        if self.in_prompt() && !centered {
            self.command_bar.render(bottom_row);
        } else {
            self.status_bar.render(bottom_row);
        }

        let new_caret_pos = if self.showing_help || self.showing_browser {
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
            self.view.caret_position()
        };
        if let Some(pos) = new_caret_pos {
            debug_assert!(pos.col <= self.terminal_size.width);
            debug_assert!(pos.row <= self.terminal_size.height);
            let _ = Terminal::move_caret_to(pos);
            let _ = if self.in_prompt() || self.mode == Mode::Insert {
                Terminal::set_cursor_bar()
            } else {
                Terminal::set_cursor_block()
            };
            let _ = Terminal::show_caret();
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

        if self.showing_dashboard && !self.in_prompt() {
            self.handle_dashboard_event(event);
            return;
        }

        if !self.in_prompt() && self.handle_modal_shortcut(&event) {
            return;
        }

        if self.in_prompt() && matches!(self.prompt_type, PromptType::Command | PromptType::Custom) {
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

        let command = if self.in_prompt() {
            Command::try_from(event).ok()
        } else {
            match self.mode {
                Mode::Insert => Command::try_from_insert(event).ok(),
                Mode::Normal => Command::try_from_normal(event, &mut self.normal_state).ok().flatten(),
                Mode::Visual => Command::try_from_visual(event).ok(),
            }
        };

        if let Some(command) = command {
            self.process_command(command);
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
                    self.view.handle_edit_command(edit_command);
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
                "q" | "quit" => self.handle_quit_command_vim(),
                "q!" | "quit!" => self.should_quit = true,
                "wq" | "x" => {
                    if self.execute_write(arg) {
                        self.should_quit = true;
                    }
                }
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
                "config" => match config::ensure_file() {
                    Ok(path) => match path.to_str() {
                        Some(path) => self.open_file(path),
                        None => self.update_message("ERR: config path is not valid UTF-8"),
                    },
                    Err(message) => self.update_message(&message),
                },
                "goto" | "go" => match arg {
                    Some(target) => self.goto_line_input(target),
                    None => self.set_prompt(PromptType::GotoLine),
                },
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

    fn goto_line_input(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
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
    

    fn handle_resize_command(&mut self, size: Size) {
        self.terminal_size = size;
        let _ = Terminal::clear_screen();
        let main = Size {
            height: size.height.saturating_sub(1),
            width: size.width,
        };
        self.view.resize(main);
        self.help.resize(main);
        self.dashboard.resize(main);
        self.browser.resize(main);
        let bar_size = Size { height: 1, width: size.width };
        self.status_bar.resize(bar_size);
        self.command_bar.resize(bar_size);
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
            match command {
                ModeCommand::EnterNormal => {
                    self.view.clear_selection();
                    self.mode = Mode::Normal;
                }
                ModeCommand::EnterInsert => self.mode = Mode::Insert,
                ModeCommand::EnterInsertAfter => {
                    self.view.handle_move_command(MoveType::Right);
                    self.mode = Mode::Insert;
                }
                ModeCommand::EnterInsertLineStart => {
                    self.view.handle_move_command(MoveType::StartOfLine);
                    self.mode = Mode::Insert;
                }
                ModeCommand::EnterInsertLineEnd => {
                    self.view.handle_move_command(MoveType::EndOfLine);
                    self.mode = Mode::Insert;
                }
                ModeCommand::EnterInsertNewlineBelow => {
                    self.view.handle_move_command(MoveType::EndOfLine);
                    self.view.handle_edit_command(InsertNewline);
                    self.mode = Mode::Insert;
                }
                ModeCommand::EnterInsertNewlineAbove => {
                    self.view.open_line_above();
                    self.mode = Mode::Insert;
                }
                ModeCommand::EnterVisual => {
                    self.view.start_selection();
                    self.mode = Mode::Visual;
                }
            }
            self.sync_mode_indicator();
    }
    fn sync_mode_indicator(&mut self) {
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
        let result = if let Some(name) = file_name {
            self.view.save_as(name)
        } else {
            self.view.save()
        };
        match result {
            Ok(()) => {
                self.update_message("File saved successfully.");
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
                self.update_message(&errors::describe("save failed, file unchanged", &target, &err));
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
                    self.view.exit_search();
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
            PromptType::GotoLine => {
                self.command_bar.set_suggestions(false);
                self.command_bar.set_custom_entries(None);
                self.command_bar
                    .set_prompt(&format!("Go to line (1-{}): ", self.view.line_count()));
                self.command_bar
                    .set_chrome("Go to line", "Enter to jump · Esc to cancel");
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
        self.prompt_type = prompt_type;
    }

    fn handle_mouse_command(&mut self, command: command::MouseCommand) {
        match command {
            command::MouseCommand::Press(row, col) => {
                self.drag_edge = None;
                self.drag_edge_since = None;
                self.view.seal_undo_group();
                self.dragging = self.is_within_view(row);
                if self.dragging {
                    let clicks = self.register_click(row, col);
                    self.view.handle_mouse_press(row, col, clicks);
                }
            }
            command::MouseCommand::Drag(row, col) => {
                if self.dragging {
                    if self.view.is_drag_edge(row, col) {
                        if self.drag_edge.is_none() {
                            self.drag_edge_since = Some(Instant::now());
                        }
                        self.drag_edge = Some((row, col));
                    } else {
                        self.drag_edge = None;
                        self.drag_edge_since = None;
                    }
                    self.view.handle_mouse_drag(row, col, self.drag_step());
                }
            }
            command::MouseCommand::Release(row, col) => {
                self.drag_edge = None;
                self.drag_edge_since = None;
                if std::mem::take(&mut self.dragging) {
                    if let Some(text) = self.view.handle_mouse_release(row, col) {
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
                        self.view.handle_mouse_press(row, col, 1);
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
        }
    }

    fn idle_wait(&self) -> Duration {
        Duration::from_millis(if self.drag_edge.is_some() { 100 } else { 500 })
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
        row < self.terminal_size.height.saturating_sub(1)
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
            Event::Key(KeyEvent { code, .. }) => {
                if Self::is_help_shortcut(&event) {
                    self.close_help();
                    return;
                }
                let page = self.help.page_size();
                match code {
                    KeyCode::Esc | KeyCode::Char('q') => self.close_help(),
                    KeyCode::Down | KeyCode::Char('j') => self.help.scroll_down(1),
                    KeyCode::Up | KeyCode::Char('k') => self.help.scroll_up(1),
                    KeyCode::PageDown | KeyCode::Char(' ' | 'f') => self.help.scroll_down(page),
                    KeyCode::PageUp | KeyCode::Char('b') => self.help.scroll_up(page),
                    KeyCode::Home | KeyCode::Char('g') => self.help.scroll_to_top(),
                    KeyCode::End | KeyCode::Char('G') => self.help.scroll_to_bottom(),
                    _ => {}
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
                    self.should_quit = true;
                    return;
                }
                if modifiers != KeyModifiers::NONE && modifiers != KeyModifiers::SHIFT {
                    return;
                }
                match code {
                    KeyCode::Char('n') => {
                        self.close_dashboard();
                        self.mode = Mode::Normal;
                        self.sync_mode_indicator();
                    }
                    KeyCode::Char('i') => {
                        self.close_dashboard();
                        self.mode = Mode::Insert;
                        self.sync_mode_indicator();
                    }
                    KeyCode::Char('e' | 'o') => self.open_browser(false, None),
                    KeyCode::Char(':') => self.set_prompt(PromptType::Command),
                    KeyCode::Char('\\') => self.set_prompt(PromptType::Custom),
                    KeyCode::Char('h') => self.open_help(),
                    KeyCode::Char('q') => self.should_quit = true,
                    KeyCode::Char(digit @ '1'..='9') => {
                        let path = digit
                            .to_digit(10)
                            .and_then(|number| number.checked_sub(1))
                            .and_then(|index| usize::try_from(index).ok())
                            .and_then(|index| self.dashboard.recent_path(index));
                        if let Some(path) = path {
                            self.open_file(&path);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn open_file(&mut self, file_name: &str) {
        if self.view.get_status().is_modified {
            self.update_message("E37: No write since last change (use :e! to discard)");
            return;
        }
        self.force_open_file(file_name);
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
            Event::Mouse(MouseEvent { kind, .. }) => match kind {
                MouseEventKind::ScrollUp => self.browser.scroll(-3),
                MouseEventKind::ScrollDown => self.browser.scroll(3),
                _ => {}
            },
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
        self.dashboard.set_recent(recent::load_existing());
        self.showing_dashboard = true;
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
                if let Err(message) = self.view.vim_search_accept(&query, forward) {
                    self.update_message(&message);
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
            (Mode::Visual, ':') => self.open_visual_command(),
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
        self.config = config;
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = Terminal::terminate();
        if self.should_quit {
            let _ = Terminal::print("Good work.\r\n");
        }
    }
}
