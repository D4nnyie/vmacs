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
}