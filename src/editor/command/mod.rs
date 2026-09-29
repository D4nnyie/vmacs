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
pub use modecommand::ModeCommand;
mod normal;
pub use normal::NormalState;
mod mouse;
pub use mouse::MouseCommand;

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
            Event::Mouse(mouse_event) => MouseCommand::try_from(mouse_event).map(Command::Mouse), // NEW
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
                    KeyCode::Home => return Ok(Self::Move(Move::BufferStart)),
                    KeyCode::End => return Ok(Self::Move(Move::BufferEnd)), 
                    _ => {}
                }
            }
            if modifiers == KeyModifiers::SHIFT {
                match code {
                    KeyCode::Left => return Ok(Self::Select(Move::Left)),
                    KeyCode::Right => return Ok(Self::Select(Move::Right)),
                    KeyCode::Up => return Ok(Self::Select(Move::Up)),
                    KeyCode::Down => return Ok(Self::Select(Move::Down)),
                    KeyCode::Home => return Ok(Self::Select(Move::StartOfLine)),
                    KeyCode::End => return Ok(Self::Select(Move::EndOfLine)),
                    _ => {}
                }
            }
            if matches!(modifiers, KeyModifiers::META | KeyModifiers::ALT) {
                match code {
                    KeyCode::Char('f') => return Ok(Self::Move(Move::WordForward)),
                    KeyCode::Char('b') => return Ok(Self::Move(Move::WordBackward)),
                    KeyCode::Char('w') => return Ok(Self::Edit(Edit::YankSelection)),
                    KeyCode::Char('<') => return Ok(Self::Move(Move::BufferStart)),
                    KeyCode::Char('>') => return Ok(Self::Move(Move::BufferEnd)),
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
        if let Ok(mv) = Move::try_from(key_event) {
            return Ok(Self::Select(mv));
        }
        Err(format!("Unsupported key in Visual mode: {code:?}"))
    }
}