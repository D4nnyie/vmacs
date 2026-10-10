use super::{ColIdx,RowIdx};

#[derive(Copy, Clone, Default)]
pub struct Position {
    pub col: ColIdx,
    pub row: RowIdx,
}