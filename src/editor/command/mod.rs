use crate::prelude::*;
use std::convert::TryFrom;
use crossterm::event::{Event, KeyEvent, KeyCode, KeyModifiers};
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

#[derive(Clone, Copy)]
pub enum Command {
    Move(Move),
    Edit(Edit),
    System(System),
    Mode(ModeCommand),
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
                    _ => {}
                }
            }
            if modifiers == KeyModifiers::NONE && matches!(code, KeyCode::Esc) {
                return Ok(Self::Mode(ModeCommand::EnterNormal));
            }
        }
        Self::try_from(event)
    }
}