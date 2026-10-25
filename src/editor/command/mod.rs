use crate::prelude::*;
use std::convert::TryFrom;
use crossterm::event::{
    Event, KeyCode::{self}, KeyEvent, KeyModifiers,
};
mod movecommand;
pub use movecommand::Move;
mod system;
pub use system::System;
mod edit;
pub use edit::Edit;
mod modecommand;
pub use modecommand::{FoldOp, ModeCommand};
mod normal;
pub use normal::NormalState;
mod mouse;
pub use mouse::MouseCommand;
mod actions;
pub use actions::{category, find_action, ACTIONS, CATEGORIES};
mod operator;
pub use operator::{Obj, Op, Target};

#[derive(Clone, Copy)]
pub enum Command {
    Move(Move),
    Select(Move),
    Edit(Edit),
    System(System),
    Mode(ModeCommand),
    Mouse(MouseCommand),
}

#[allow(clippy::as_conversions)]
impl TryFrom<Event> for Command {
    type Error = String;
    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match event {
            Event::Key(key_event) => Edit::try_from(key_event)
                .map(Command::Edit)
                .or_else(|_| Move::try_from(key_event).map(Command::Move))
                .or_else(|_| System::try_from(key_event).map(Command::System))
                .map_err(|_err| format!("Event not supported: {key_event:?}")),
            Event::Resize(width_u16, height_u16) => Ok(Self::System(System::Resize(Size {
                height: height_u16 as usize,
                width: width_u16 as usize,
            }))),
            Event::Mouse(mouse_event) => MouseCommand::try_from(mouse_event).map(Command::Mouse),
            _ => Err(format!("Event not supported: {event:?}")),
        }
    }
}

impl Command {
    pub fn try_from_insert(event: Event) -> Result<Self, String> {
        if let Event::Key(key_event) = event {
            let KeyEvent { code, modifiers, .. } = key_event;

            if modifiers == KeyModifiers::CONTROL {
                match code {
                    KeyCode::Char('a') => return Ok(Self::Move(Move::StartOfLine)),
                    KeyCode::Char('e') => return Ok(Self::Move(Move::EndOfLine)),
                    KeyCode::Char('f') => return Ok(Self::Move(Move::Right)),
                    KeyCode::Char('b') => return Ok(Self::Move(Move::Left)),
                    KeyCode::Char('n') => return Ok(Self::Move(Move::Down)),
                    KeyCode::Char('p') => return Ok(Self::Move(Move::Up)),
                    KeyCode::Char('d') => return Ok(Self::Edit(Edit::Delete)),
                    KeyCode::Char('k') => return Ok(Self::Edit(Edit::DeleteToLineEnd)),
                    KeyCode::Char('y') => return Ok(Self::Edit(Edit::PasteInline)),
                    KeyCode::Char('h') => return Ok(Self::Edit(Edit::DeleteBackward)),
                    KeyCode::Char('r') => return Ok(Self::Edit(Edit::Redo)),
                    KeyCode::Char('z') => return Ok(Self::Edit(Edit::Undo)),
                    KeyCode::Home => return Ok(Self::Move(Move::BufferStart)),
                    KeyCode::End => return Ok(Self::Move(Move::BufferEnd)), 
                    _ => {}
                }
            }
            if let Ok(motion) = Move::try_from_shifted(key_event) {
                return Ok(Self::Select(motion));
            }
            if matches!(modifiers, KeyModifiers::META | KeyModifiers::ALT) {
                match code {
                    KeyCode::Char('f') => return Ok(Self::Move(Move::WordForward)),
                    KeyCode::Char('b') => return Ok(Self::Move(Move::WordBackward)),
                    KeyCode::Char('w') => return Ok(Self::Edit(Edit::YankSelection)),
                    KeyCode::Char('<') => return Ok(Self::Move(Move::BufferStart)),
                    KeyCode::Char('>') => return Ok(Self::Move(Move::BufferEnd)),
                    KeyCode::Char('c') => return Ok(Self::Edit(Edit::YankSelectionSystem)),
                    KeyCode::Char('x') => return Ok(Self::System(System::CommandPrompt)),
                    _ => {}
                }
            }
            if modifiers == KeyModifiers::NONE && matches!(code, KeyCode::Esc) {
                return Ok(Self::Mode(ModeCommand::EnterNormal));
            }
        }
        Self::try_from(event)
    }

    pub fn try_from_visual(event: Event) -> Result<Self, String> {
        let Event::Key(key_event) = event else {
            return Self::try_from(event);
        };
        let KeyEvent { code, modifiers, .. } = key_event;

        if matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
            match code {
                KeyCode::Char('h') => return Ok(Self::Select(Move::Left)),
                KeyCode::Char('l') => return Ok(Self::Select(Move::Right)),
                KeyCode::Char('k') => return Ok(Self::Select(Move::Up)),
                KeyCode::Char('j') => return Ok(Self::Select(Move::Down)),
                KeyCode::Char('0') => return Ok(Self::Select(Move::StartOfLine)),
                KeyCode::Char('$') => return Ok(Self::Select(Move::EndOfLine)),
                KeyCode::Char('w') => return Ok(Self::Select(Move::WordForward)),
                KeyCode::Char('b') => return Ok(Self::Select(Move::WordBackward)),
                KeyCode::Char('d') | KeyCode::Char('x') => return Ok(Self::Edit(Edit::DeleteSelection)),
                KeyCode::Char('y') => return Ok(Self::Edit(Edit::YankSelection)),
                KeyCode::Char('u') => return Ok(Self::Edit(Edit::LowercaseSelection)),
                KeyCode::Char('U') => return Ok(Self::Edit(Edit::UppercaseSelection)),
                KeyCode::Char('>') => return Ok(Self::Edit(Edit::IndentSelection)),
                KeyCode::Char('<') => return Ok(Self::Edit(Edit::DedentSelection)),
                KeyCode::Char('v') | KeyCode::Esc => return Ok(Self::Mode(ModeCommand::EnterNormal)),
                KeyCode::Backspace => return  Ok(Self::Edit(Edit::DeleteBackward)),
                KeyCode::Char('g') => return Ok(Self::Select(Move::BufferStart)),
                KeyCode::Char('G') => return Ok(Self::Select(Move::BufferEnd)),
                KeyCode::Char('{') => return Ok(Self::Select(Move::ParagraphBackward)),
                KeyCode::Char('}') => return Ok(Self::Select(Move::ParagraphForward)),
                KeyCode::Char('c') => return Ok(Self::Edit(Edit::YankSelectionSystem)),
                KeyCode::Char('H') => return Ok(Self::Select(Move::ScreenTop)),
                KeyCode::Char('M') => return Ok(Self::Select(Move::ScreenMiddle)),
                KeyCode::Char('L') => return Ok(Self::Select(Move::ScreenBottom)),
                _ => {}
            }
        }
        if modifiers == KeyModifiers::CONTROL{
            match code {
                KeyCode::Char('h') => return Ok(Self::Edit(Edit::DeleteBackward)),
                KeyCode::Home => return Ok(Self::Select(Move::BufferStart)),
                KeyCode::End => return Ok(Self::Select(Move::BufferEnd)),
            _   => {}
            }
        }
        if matches!(modifiers, KeyModifiers::META | KeyModifiers::ALT) {
            match code {
                KeyCode::Char('c') => return Ok(Self::Edit(Edit::YankSelectionSystem)),
                _ => {}
            }
        }
        if let Ok(motion) = Move::try_from_shifted(key_event) {
            return Ok(Self::Select(motion));
        }
        if let Ok(mv) = Move::try_from(key_event) {
            return Ok(Self::Select(mv));
        }
        Err(format!("Unsupported key in Visual mode: {code:?}"))
    }

    pub fn try_from_visual_with(event: Event, state: &mut NormalState) -> Result<Option<Self>, String> {
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event {
            let plain = matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT);
            if let Some(op) = state.take_find() {
                return match code {
                    KeyCode::Char(ch) if plain => Ok(Some(Self::Select(Move::Find {
                        ch,
                        forward: matches!(op, 'f' | 't'),
                        till: matches!(op, 't' | 'T'),
                    }))),
                    KeyCode::Esc => Ok(None),
                    _ => Err(format!("Unsupported find target: {code:?}")),
                };
            }
            if plain {
                match code {
                    KeyCode::Char(c @ ('f' | 'F' | 't' | 'T')) => {
                        state.begin_find(c);
                        return Ok(None);
                    }
                    KeyCode::Char('e') => return Ok(Some(Self::Select(Move::WordEnd))),
                    KeyCode::Char('^') => return Ok(Some(Self::Select(Move::FirstNonBlank))),
                    KeyCode::Char('%') => return Ok(Some(Self::Select(Move::MatchingBracket))),
                    KeyCode::Char('W') => return Ok(Some(Self::Select(Move::BigWordForward))),
                    KeyCode::Char('B') => return Ok(Some(Self::Select(Move::BigWordBackward))),
                    KeyCode::Char('E') => return Ok(Some(Self::Select(Move::BigWordEnd))),
                    KeyCode::Tab => return Ok(Some(Self::Select(Move::JumpForward))),
                    _ => {}
                }
            }
            if modifiers == KeyModifiers::CONTROL {
                match code {
                    KeyCode::Char('d') => return Ok(Some(Self::Select(Move::HalfPageDown))),
                    KeyCode::Char('u') => return Ok(Some(Self::Select(Move::HalfPageUp))),
                    KeyCode::Char('o') => return Ok(Some(Self::Select(Move::JumpBack))),
                    _ => {}
                }
            }
        }
        Self::try_from_visual(event).map(Some)
    }
}