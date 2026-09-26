    use crate::prelude::*;
    use crossterm::event::{read, Event, KeyEvent, KeyEventKind};
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
    mod filetype;
    use annotatedstring::AnnotatedString;
    use documentstatus::DocumentStatus;
    use filetype::FileType;
    use line::Line;
    use terminal::Terminal;
    use uicomponents::{CommandBar, MessageBar, StatusBar, UIComponent, View};
    use mode::Mode;
    use command::NormalState;

    use self::command::{
        Command::{self, Edit, Move as MoveCmd, System},
        Edit::InsertNewline,
        Move as MoveType,
        ModeCommand,
        System::{CommandPrompt, Dismiss, Quit, Resize, Save, SaveAs, Search},
    };


    const QUIT_TIMES: u8 = 3;

    #[derive(Eq, PartialEq, Default)]
    enum PromptType {
        Search,
        Save,
        SaveAs,
        Command, // Vim
        #[default]
        None,
    }

    impl PromptType {
        fn is_none(&self) -> bool {
            *self == Self::None
        }
    }

    #[derive(Default)]
    pub struct Editor {
        should_quit: bool,
        view: View,
        status_bar: StatusBar,
        message_bar: MessageBar,
        command_bar: CommandBar,
        prompt_type: PromptType,
        terminal_size: Size,
        title: String,
        quit_times: u8,
        mode: Mode,
        normal_state: NormalState,
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
            let size = Terminal::size().unwrap_or_default();
            editor.handle_resize_command(size);
            editor.update_message("HELP: C^q = quit :q = quit");

            let args: Vec<String> = env::args().collect();
            if let Some(file_name) = args.get(1) {
                debug_assert!(!file_name.is_empty());
                if editor.view.load(file_name).is_err() {
                    editor.update_message(&format!("ERR: Could not open file: {file_name}"));
                }
            }
            editor.refresh_status();
            editor.sync_mode_indicator();
            Ok(editor)
        }

        
        pub fn run(&mut self) {
            loop {
                self.refresh_screen();
                if self.should_quit {
                    break;
                }
                match read() {
                    Ok(event) => self.evaluate_event(event),
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

        fn refresh_screen(&mut self) {
            if self.terminal_size.height == 0 || self.terminal_size.width == 0 {
                return;
            }
            let bottom_bar_row = self.terminal_size.height.saturating_sub(1);
            let _ = Terminal::hide_caret();
            if self.in_prompt() {
                self.command_bar.render(bottom_bar_row);
            } else {
                self.message_bar.render(bottom_bar_row);
            }
            if self.terminal_size.height > 1 {
                self.status_bar
                    .render(self.terminal_size.height.saturating_sub(2));
            }
            if self.terminal_size.height > 2 {
                self.view.render(0);
            }
            let new_caret_pos = if self.in_prompt() {
                Position {
                    row: bottom_bar_row,
                    col: self.command_bar.caret_position_col(),
                }
            } else {
                self.view.caret_position()
            };
            debug_assert!(new_caret_pos.col <= self.terminal_size.width);
            debug_assert!(new_caret_pos.row <= self.terminal_size.height);

            let _ = Terminal::move_caret_to(new_caret_pos);
            let _ = Terminal::show_caret();
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
                _ => false,
            };
            if !should_process {
                return;
            }

            let command = if self.in_prompt() {
                Command::try_from(event).ok()
            } else {
                match self.mode {
                    Mode::Insert => Command::try_from_insert(event).ok(),
                    Mode::Normal => Command::try_from_normal(event, &mut self.normal_state)
                        .ok()
                        .flatten(),
                }
            };

            if let Some(command) = command {
                self.process_command(command);
            }
        }
        


        fn process_command(&mut self, command: Command) {
                if let System(Resize(size)) = command {
                    self.handle_resize_command(size);
                    return;
                }
                match self.prompt_type {
                    PromptType::Search => self.process_command_during_search(command),
                    PromptType::Save => self.process_command_during_save(command),
                    PromptType::SaveAs => self.process_command_during_save(command),
                    PromptType::Command => self.process_command_during_command_prompt(command),
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
                    Edit(edit_command) => self.view.handle_edit_command(edit_command),
                    MoveCmd(move_command) => self.view.handle_move_command(move_command),
                    Command::Mode(mode_command) => self.handle_mode_command(mode_command),
                }
        }

    
        fn process_command_during_command_prompt(&mut self, command: Command) {
            match command {
                System(Dismiss) => {
                    self.set_prompt(PromptType::None);
                }
                Edit(InsertNewline) => {
                    let input = self.command_bar.value();
                    self.set_prompt(PromptType::None);
                    self.execute_command_line(&input);
                }
                Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
                System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Command::Mode(_) => {}
            }
        }

        fn execute_command_line(&mut self, input: &str) {
                let trimmed = input.trim();
                let mut parts = trimmed.splitn(2, char::is_whitespace);
                let cmd = parts.next().unwrap_or("");
                let arg = parts.next().map(str::trim).filter(|s| !s.is_empty());

                match cmd {
                    "w" => {
                        self.execute_write(arg);
                    }
                    "q" => self.handle_quit_command_vim(),
                    "q!" => self.should_quit = true,
                    "wq" | "x" => {
                        if self.execute_write(arg) {
                            self.should_quit = true;
                        }
                    }
                    "" => {}
                    other => self.update_message(&format!("E492: Not an editor command: {other}")),
                }
        }
        
        fn execute_write(&mut self, arg: Option<&str>) -> bool {
            if let Some(file_name) = arg {
                self.save(Some(file_name));
                true
            } else if self.view.is_file_loaded() {
                self.save(None);
                true
            } else {
                self.set_prompt(PromptType::Save);
                false
            }
        }
        

        fn handle_resize_command(&mut self, size: Size) {
            self.terminal_size = size;
            self.view.resize(Size {
                height: size.height.saturating_sub(2),
                width: size.width,
            });
            let bar_size = Size {
                height: 1,
                width: size.width,
            };
            self.message_bar.resize(bar_size);
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
                    ModeCommand::EnterNormal => self.mode = Mode::Normal,
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
                        self.view.handle_move_command(MoveType::StartOfLine);
                        self.view.handle_edit_command(InsertNewline);
                        self.view.handle_move_command(MoveType::Up);
                        self.mode = Mode::Insert;
                    }
                }
                self.sync_mode_indicator();
        }
        fn sync_mode_indicator(&mut self) {
                self.status_bar.update_mode(match self.mode {
                    Mode::Normal => "NORMAL",
                    Mode::Insert => "INSERT",
                });
        }
        fn process_command_during_save(&mut self, command: Command) {
                match command {
                    System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Command::Mode(_) => {}
                    System(Dismiss) => {
                        self.set_prompt(PromptType::None);
                        self.update_message("Save aborted.");
                    }
                    Edit(InsertNewline) => {
                        let file_name = self.command_bar.value();
                        self.save(Some(&file_name));
                        self.set_prompt(PromptType::None);
                    }
                    Edit(edit_command) => self.command_bar.handle_edit_command(edit_command),
                }
        }
        fn save(&mut self, file_name: Option<&str>) {
            let result = if let Some(name) = file_name {
                self.view.save_as(name)
            } else {
                self.view.save()
            };
            if result.is_ok() {
                self.update_message("File saved successfully.");
            } else {
                self.update_message("Error writing file!");
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
                    System(Quit | Resize(_) | Search | Save | SaveAs | CommandPrompt) | MoveCmd(_) | Command::Mode(_) => {}
                }
        }

        fn update_message(&mut self, new_message: &str) {
            self.message_bar.update_message(new_message);
        }

        fn in_prompt(&self) -> bool {
            !self.prompt_type.is_none()
        }

        fn set_prompt(&mut self, prompt_type: PromptType) {
            match prompt_type {
                PromptType::None => self.message_bar.set_needs_redraw(true), 
                PromptType::Save => self.command_bar.set_prompt("Save as: "),
                PromptType::SaveAs => self.command_bar.set_prompt("Save as: "),
                PromptType::Search => {
                    self.view.enter_search();
                    self.command_bar
                        .set_prompt("Search (Esc to cancel, Arrows to navigate): ");
                }
                PromptType::Command => self.command_bar.set_prompt(":"),
            }
            self.command_bar.clear_value();
            self.prompt_type = prompt_type;
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