use crossterm::event::{
    KeyCode::{self, Down, End, Home, Left, PageDown, PageUp, Right, Up},
    KeyEvent, KeyModifiers,
};

#[derive(Clone, Copy)]
pub enum Move {
    PageUp,
    PageDown,
    StartOfLine,
    EndOfLine,
    Up,
    Left,
    Right,
    Down,
    WordForward,
    WordBackward,
    ParagraphForward,
    ParagraphBackward,
    WordEnd,
    FirstNonBlank,
    LastNonBlank,
    MatchingBracket,
    HalfPageUp,
    HalfPageDown,
    Find { ch: char, forward: bool, till: bool },
    BufferStart,
    BufferEnd,
    SmartHome,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    ViewUp,
    ViewDown,
    ViewTop,
    ViewCenter,
    ViewBottom,
    JumpBack,
    JumpForward,
    BigWordForward,
    BigWordBackward,
    BigWordEnd,
    ViewLeft,
    ViewRight,
    ViewHalfLeft,
    ViewHalfRight,
    ViewColStart,
    ViewColEnd,
    NextLineStart,
    PrevLineStart,
}

impl TryFrom<KeyEvent> for Move {
    type Error = String;
    fn try_from(event: KeyEvent) -> Result<Self, Self::Error> {
        let KeyEvent {
            code, modifiers, ..
        } = event;

        if modifiers == KeyModifiers::NONE {
            match code {
                Up => Ok(Self::Up),
                Down => Ok(Self::Down),
                Left => Ok(Self::Left),
                Right => Ok(Self::Right),
                PageDown => Ok(Self::PageDown),
                PageUp => Ok(Self::PageUp),
                Home => Ok(Self::SmartHome),
                End => Ok(Self::EndOfLine),
                _ => Err(format!("Unsupported code: {code:?}")),
            }
        } else if modifiers == KeyModifiers::CONTROL {
            match code {
                Left => Ok(Self::WordBackward),
                Right => Ok(Self::WordForward),
                Up => Ok(Self::ViewUp),
                Down => Ok(Self::ViewDown),
                _ => Err(format!("Unsupported CONTROL+{code:?} movement")),
            }
        } else if modifiers == KeyModifiers::ALT || modifiers == KeyModifiers::META {
            match code {
                Up => Ok(Self::ParagraphBackward),
                Down => Ok(Self::ParagraphForward),
                Left => Ok(Self::Left),
                Right => Ok(Self::Right),
                _ => Err(format!("Unsupported ALT+{code:?} movement")),
            }
        } else {
            Err(format!(
                "Unsupported key code {code:?} or modifier {modifiers:?}"
            ))
        }
    }
}

impl Move {

    pub const fn is_inclusive(self) -> bool {
        matches!(
            self,
            Self::WordEnd | Self::LastNonBlank | Self::MatchingBracket | Self::BigWordEnd | Self::Find { .. }
        )
    }

    pub const fn is_jump(self) -> bool {
        matches!(
            self,
            Self::BufferStart
                | Self::BufferEnd
                | Self::ParagraphForward
                | Self::ParagraphBackward
                | Self::MatchingBracket
                | Self::ScreenTop
                | Self::ScreenMiddle
                | Self::ScreenBottom
        )
    }

    pub const fn is_viewport(self) -> bool {
        matches!(
            self,
            Self::ViewUp | Self::ViewDown | Self::ViewTop | Self::ViewCenter | Self::ViewBottom
                | Self::ViewLeft | Self::ViewRight | Self::ViewHalfLeft | Self::ViewHalfRight
                | Self::ViewColStart | Self::ViewColEnd
        )
    }

    pub fn try_from_shifted(event: KeyEvent) -> Result<Self, String> {
        use KeyCode::{Down, End, Home, Left, PageDown, PageUp, Right, Up};
        let KeyEvent { code, modifiers, .. } = event;
        if !modifiers.contains(KeyModifiers::SHIFT) {
            return Err("not a shifted key".to_string());
        }
        let rest = modifiers - KeyModifiers::SHIFT;
        let found = if rest == KeyModifiers::NONE {
            match code {
                Up => Some(Self::Up),
                Down => Some(Self::Down),
                Left => Some(Self::Left),
                Right => Some(Self::Right),
                Home => Some(Self::SmartHome),
                End => Some(Self::EndOfLine),
                PageUp => Some(Self::PageUp),
                PageDown => Some(Self::PageDown),
                _ => None,
            }
        } else if rest == KeyModifiers::CONTROL {
            match code {
                Left => Some(Self::WordBackward),
                Right => Some(Self::WordForward),
                Home => Some(Self::BufferStart),
                End => Some(Self::BufferEnd),
                _ => None,
            }
        } else if rest == KeyModifiers::ALT || rest == KeyModifiers::META {
            match code {
                Left => Some(Self::ParagraphBackward),
                Right => Some(Self::ParagraphForward),
                _ => None,
            }
        } else {
            None
        };
        found.ok_or_else(|| format!("Unsupported shifted key {code:?} {modifiers:?}"))
    }
}