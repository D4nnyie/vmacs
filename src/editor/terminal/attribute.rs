use crossterm::style::Color;

use super::super::AnnotationType;

use super::Terminal;

pub struct Attribute {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

impl Attribute {
    fn fg(r: u8, g: u8, b: u8) -> Self {
        Self {
            foreground: Some(Color::Rgb { r, g, b }),
            background: None,
            bold: false,
            italic: false,
        }
    }
    fn with_bg(mut self, r: u8, g: u8, b: u8) -> Self {
        self.background = Some(Color::Rgb { r, g, b });
        self
    }
    fn styled(mut self, bold: bool, italic: bool) -> Self {
        self.bold = bold;
        self.italic = italic;
        self
    }
}

impl From<AnnotationType> for Attribute {
    fn from(annotation_type: AnnotationType) -> Self {
        match annotation_type {
            AnnotationType::Match => Self::fg(0xe6, 0xed, 0xf3).with_bg(0x26, 0x32, 0x38),
            AnnotationType::SelectedMatch => Self::fg(0xf5, 0xf5, 0xf5).with_bg(0x5a, 0x4a, 0x26),
            AnnotationType::Selection => Self::fg(0xe6, 0xed, 0xf3).with_bg(0x26, 0x3b, 0x59),
            AnnotationType::Number => Self::fg(0xf2, 0x8f, 0x79),

            AnnotationType::Keyword => Self::fg(0xc7, 0x92, 0xea).styled(true, true),
            AnnotationType::Function => Self::fg(0x82, 0xaa, 0xff),
            AnnotationType::Module => Self::fg(0x89, 0xdd, 0xff),
            AnnotationType::Attribute => Self::fg(0x7f, 0xdb, 0xca),
            AnnotationType::Type => Self::fg(0xa6, 0xe3, 0xa1),

            AnnotationType::Scope => Self {
                foreground: None,
                background: Some(Terminal::SCOPE_BG),
                bold: false,
                italic: false,
            },
            AnnotationType::Bracket => Self::fg(0x82, 0xaa, 0xff)
                .with_bg(0x2a, 0x3b, 0x59)
                .styled(true, false),

            AnnotationType::Property => Self::fg(0x8f, 0xb8, 0xd8),
            AnnotationType::Parameter => Self::fg(0xe5, 0xc0, 0x7b),
            AnnotationType::Operator => Self::fg(0x89, 0xdd, 0xff),
            AnnotationType::Punctuation => Self::fg(0x71, 0x80, 0x96),
            AnnotationType::Builtin => Self::fg(0xc7, 0x92, 0xea).styled(false, true),
            AnnotationType::KnownValue => Self::fg(0xf6, 0xc8, 0x5f),
            AnnotationType::Char => Self::fg(0xf2, 0xcc, 0x60),
            AnnotationType::LifetimeSpecifier => Self::fg(0x7f, 0xdb, 0xca),
            AnnotationType::Comment => Self::fg(0x63, 0x75, 0x8a),
            AnnotationType::String => Self::fg(0xc3, 0xe8, 0x8d),
        }
    }
}


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
        assert_eq!(a.foreground, Some(Color::Rgb { r: 0x82, g: 0xaa, b: 0xff }));
        assert_eq!(a.background, Some(Color::Rgb { r: 0x2a, g: 0x3b, b: 0x59 }));
    }

    #[test]
    fn keywords_are_bold_italic() {
        let a = Attribute::from(AnnotationType::Keyword);
        assert!(a.bold && a.italic);
    }
}
