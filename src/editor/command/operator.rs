#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Delete,
    Yank,
    Change,
    Indent,
    Dedent,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obj {
    Word,
    BigWord,
    Quote(char),
    Pair(char),
    Paragraph,
}

#[derive(Clone, Copy)]
pub enum Target {
    Motion(super::Move),
    Line,
    Object { obj: Obj, around: bool },
}