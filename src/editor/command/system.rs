use crate::prelude::*;
use crossterm::event::{
    KeyCode::{self, Char},
    KeyEvent, KeyModifiers,
};

#[derive(Clone, Copy)]
pub enum System {
    Save,
    SaveAs,
    Resize(Size),
    Quit,
    Dismiss,
    Search,
    CommandPrompt,
}

impl TryFrom<KeyEvent> for System {
    type Error = String;
    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        let KeyEvent {
            code, modifiers, ..
        } = event;

        if modifiers == KeyModifiers::CONTROL {
            match code { //Main universal keybinds
                Char('q') => Ok(Self::Quit),
                Char('x') => Ok(Self::Save),
                Char('w') => Ok(Self::SaveAs),
                Char('s') => Ok(Self::Search),
                _ => Err(format!("Unsupported CONTROL+{code:?} combination")),
            }
        } else if modifiers == KeyModifiers::NONE && matches!(code, KeyCode::Esc) {
            Ok(Self::Dismiss)
        } else {
            Err(format!(
                "Unsupported key code {code:?} or modifier {modifiers:?}"
            ))
        }
    }
}