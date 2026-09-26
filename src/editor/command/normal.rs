use super::{Command, Edit, ModeCommand, Move, System};
use crossterm::event::{
    Event, KeyCode::{self, Char},
    KeyEvent, KeyModifiers,
};

#[derive(Default)]
pub struct NormalState {
    pending_op: Option<char>,
}

impl Command {
    pub fn try_from_normal(event: Event, state: &mut NormalState) -> Result<Option<Self>, String> {
        let Event::Key(key_event) = event else {
            return Command::try_from(event).map(Some);
        };
        let KeyEvent { code, modifiers, .. } = key_event;

        if let Some(op) = state.pending_op {
            state.pending_op = None;
            return match (op, code, modifiers) {
                ('d', Char('d'), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Some(Self::Edit(Edit::DeleteLine))),
                ('d', Char('w'), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                ('d', Char('$'), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                ('y', Char('y'), KeyModifiers::NONE | KeyModifiers::SHIFT) => Ok(Some(Self::Edit(Edit::YankLine))),
                _ => Err(format!("Unsupported operator+motion: {op}{code:?}")),
            };
        }

        if matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) {
            match code {
                Char('h') => return Ok(Some(Self::Move(Move::Left))),
                Char('l') => return Ok(Some(Self::Move(Move::Right))),
                Char('k') => return Ok(Some(Self::Move(Move::Up))),
                Char('j') => return Ok(Some(Self::Move(Move::Down))),
                Char('0') => return Ok(Some(Self::Move(Move::StartOfLine))),
                Char('$') => return Ok(Some(Self::Move(Move::EndOfLine))),
                Char('w') => return Ok(Some(Self::Move(Move::WordForward))),
                Char('b') => return Ok(Some(Self::Move(Move::WordBackward))),
                Char('x') => return Ok(Some(Self::Edit(Edit::DeleteCharYank))),
                Char('D') => return Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                Char('d') => {
                    state.pending_op = Some('d');
                    return Ok(None);
                }
                Char('y') => {
                    state.pending_op = Some('y');
                    return Ok(None);
                }
                Char('p') => return Ok(Some(Self::Edit(Edit::Paste))),
                Char('P') => return Ok(Some(Self::Edit(Edit::PasteBefore))),
                Char('i') => return Ok(Some(Self::Mode(ModeCommand::EnterInsert))),
                Char('a') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertAfter))),
                Char('I') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertLineStart))),
                Char('A') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertLineEnd))),
                Char('o') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertNewlineBelow))),
                Char('O') => return Ok(Some(Self::Mode(ModeCommand::EnterInsertNewlineAbove))),
                Char(':') => return Ok(Some(Self::System(System::CommandPrompt))),
                KeyCode::Esc => return Ok(Some(Self::System(System::Dismiss))),
                _ => {}
            }
        }

        // Emacs bleed-through, even in Normal mode
        if modifiers == KeyModifiers::CONTROL {
            match code {
                Char('s') => return Ok(Some(Self::System(System::Search))),
                Char('x') => return Ok(Some(Self::System(System::Save))),
                Char('w') => return Ok(Some(Self::System(System::SaveAs))),
                Char('q') => return Ok(Some(Self::System(System::Quit))),
                Char('d') => return Ok(Some(Self::Edit(Edit::Delete))),
                Char('k') => return Ok(Some(Self::Edit(Edit::DeleteToLineEnd))),
                Char('y') => return Ok(Some(Self::Edit(Edit::PasteInline))),
                Char('n') => return Ok(Some(Self::Move(Move::Up))),
                Char('p') => return Ok(Some(Self::Move(Move::Down))),
                Char('f') => return Ok(Some(Self::Move(Move::Right))),
                Char('b') => return Ok(Some(Self::Move(Move::Left))),
                Char('a') => return Ok(Some(Self::Move(Move::StartOfLine))),
                Char('e') => return Ok(Some(Self::Move(Move::EndOfLine))),
                _ => {}
            }
        }

        if matches!(modifiers, KeyModifiers::META | KeyModifiers::ALT) {
            match code {
                Char('f') => return Ok(Some(Self::Move(Move::WordForward))),
                Char('b') => return Ok(Some(Self::Move(Move::WordBackward))),
                _ => {}
            }
        }

        // Fallback: arrow keys, Page Up/Down, Home/End still work in Normal mode,
        // same as they always did, even though they're not vim bindings.
        if let Ok(mv) = Move::try_from(key_event) {
            return Ok(Some(Self::Move(mv)));
        }
        if let Ok(sys) = System::try_from(key_event) {
            return Ok(Some(Self::System(sys)));
        }

        Err(format!("Unsupported key in Normal mode: {code:?}"))
    }
}