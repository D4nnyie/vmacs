use crossterm::style::Color;
use super::super::{theme as t, AnnotationType};
use super::Terminal;

pub struct Attribute {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

impl Attribute {
    const fn fg(c: Color) -> Self { Self { foreground: Some(c), background: None, bold: false, italic: false } }
    fn with_bg(mut self, c: Color) -> Self { self.background = Some(c); self }
    fn styled(mut self, bold: bool, italic: bool) -> Self { self.bold = bold; self.italic = italic; self }
}

impl From<AnnotationType> for Attribute {
    fn from(a: AnnotationType) -> Self {
        use AnnotationType as A;
        match a {
            A::Match => Self::fg(t::TEXT_BRIGHT).with_bg(t::MATCH_BG),
            A::SelectedMatch => Self::fg(t::MATCH_SELECTED_FG).with_bg(t::MATCH_SELECTED_BG),
            A::Selection => Self::fg(t::TEXT_BRIGHT).with_bg(t::SEL_BG),
            A::Number => Self::fg(t::SYN_NUMBER),
            A::Keyword => Self::fg(t::PURPLE).styled(true, true),
            A::Function => Self::fg(t::BLUE),
            A::Module => Self::fg(t::SYN_MODULE),
            A::Attribute => Self::fg(t::SYN_ATTRIBUTE),
            A::Type => Self::fg(t::SYN_TYPE),
            A::Scope => Self { foreground: None, background: Some(Terminal::SCOPE_BG), bold: false, italic: false },
            A::Bracket => Self::fg(t::BLUE).with_bg(t::SURFACE_ACTIVE).styled(true, false),
            A::Property => Self::fg(t::SYN_PROPERTY),
            A::Parameter => Self::fg(t::SYN_PARAMETER),
            A::Operator => Self::fg(t::SYN_MODULE),
            A::Punctuation => Self::fg(t::SYN_PUNCT),
            A::Builtin => Self::fg(t::PURPLE).styled(false, true),
            A::KnownValue => Self::fg(t::AMBER),
            A::Char => Self::fg(t::SYN_CHAR),
            A::LifetimeSpecifier => Self::fg(t::SYN_ATTRIBUTE),
            A::Comment => Self::fg(t::TEXT_DIM),
            A::String => Self::fg(t::GREEN),
            A::Border => Self::fg(t::BLUE_MUTED),
            A::Flash => Self::fg(t::TEXT_BRIGHT).with_bg(t::FLASH_BG).styled(true, false),
            A::FlashSoft => Self::fg(t::TEXT_BRIGHT).with_bg(t::FLASH_SOFT_BG),
        }
    }
}
// tests: replace the RGB literals in `bracket_is_bold_blue_on_slate` with t::BLUE / t::SURFACE_ACTIVE.


#[cfg(test)]
mod tests {
    use super::super::Terminal;
    use super::*;

    #[test]
    fn scope_only_sets_a_background() {
        let a = Attribute::from(AnnotationType::Scope);
        assert_eq!(a.background, Some(Terminal::SCOPE_BG));
        assert!(a.foreground.is_none() && !a.bold && !a.italic);
    }

    #[test]
    fn bracket_is_bold_blue_on_slate() {
        let a = Attribute::from(AnnotationType::Bracket);
        assert!(a.bold);
        assert_eq!(a.foreground, Some(t::BLUE));
        assert_eq!(a.background, Some(t::SURFACE_ACTIVE));
    }

    #[test]
    fn keywords_are_bold_italic() {
        let a = Attribute::from(AnnotationType::Keyword);
        assert!(a.bold && a.italic);
    }
}
