#[derive(Clone, Copy)]
pub enum ModeCommand {
    EnterNormal,
    EnterInsert,             // vim `i`
    EnterInsertAfter,        // vim `a`
    EnterInsertLineStart,    // vim `I`
    EnterInsertLineEnd,      // vim `A`
    EnterInsertNewlineBelow, // vim `o`
    EnterInsertNewlineAbove, // vim `O`
    EnterVisual,             // vim 'v'
    Fold(FoldOp),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldOp {
    Toggle,
    Open,
    Close,
    OpenAll,
    CloseAll,
    Next,
    Prev,
}