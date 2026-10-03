use crate::prelude::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

#[derive(Clone, Copy)]
pub enum MouseCommand {
    Press(RowIdx, ColIdx),
    Drag(RowIdx, ColIdx),
    Release(RowIdx, ColIdx),
    ScrollUp,
    ScrollDown,
}

impl TryFrom<MouseEvent> for MouseCommand {
    type Error = String;
    #[allow(clippy::as_conversions)]
    fn try_from(event: MouseEvent) -> Result<Self, Self::Error> {
        let MouseEvent { kind, column, row, .. } = event;
        match kind {
            MouseEventKind::Down(MouseButton::Left) => Ok(Self::Press(row as usize, column as usize)),
            MouseEventKind::Drag(MouseButton::Left) => Ok(Self::Drag(row as usize, column as usize)),
            MouseEventKind::Up(MouseButton::Left) => Ok(Self::Release(row as usize, column as usize)),
            MouseEventKind::ScrollUp => Ok(Self::ScrollUp),
            MouseEventKind::ScrollDown => Ok(Self::ScrollDown),
            _ => Err(format!("Unsupported mouse event: {kind:?}")),
        }
    }
}