#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Normal // vim-like default; flip to Insert if you'd rather start typing immediately
    }
}