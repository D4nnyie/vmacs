use crate::prelude::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[derive(Clone, Copy)]
pub enum MouseCommand {
    Press(RowIdx, ColIdx),
    Drag(RowIdx, ColIdx),
    Release(RowIdx, ColIdx),
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    ScrollUpFast,
    ScrollDownFast,
    MiddleClick(RowIdx, ColIdx),
}

impl TryFrom<MouseEvent> for MouseCommand {
    type Error = String;
    #[allow(clippy::as_conversions)]
    fn try_from(event: MouseEvent) -> Result<Self, Self::Error> {
        let MouseEvent { kind, column, row, modifiers } = event;
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        match kind {
            MouseEventKind::Down(MouseButton::Left) => Ok(Self::Press(row as usize, column as usize)),
            MouseEventKind::Drag(MouseButton::Left) => Ok(Self::Drag(row as usize, column as usize)),
            MouseEventKind::Up(MouseButton::Left) => Ok(Self::Release(row as usize, column as usize)),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = kind == MouseEventKind::ScrollUp;
                Ok(match (ctrl, shift, up) {
                    (true, _, true) => Self::ScrollUpFast,
                    (true, _, false) => Self::ScrollDownFast,
                    (false, true, true) => Self::ScrollLeft,
                    (false, true, false) => Self::ScrollRight,
                    (false, false, true) => Self::ScrollUp,
                    (false, false, false) => Self::ScrollDown,
                })
            }
            MouseEventKind::ScrollLeft => Ok(Self::ScrollLeft),
            MouseEventKind::ScrollRight => Ok(Self::ScrollRight),
            MouseEventKind::Down(MouseButton::Middle) => {
                Ok(Self::MiddleClick(row as usize, column as usize))
            }
            _ => Err(format!("Unsupported mouse event: {kind:?}")),
        }
    }
}