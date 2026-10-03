#[derive(Debug)]
#[allow(dead_code)]
pub struct LanguageConfig {
    pub name: &'static str,
    pub extensions: &'static [&'static str],

    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub known_values: &'static [&'static str],

    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,

    pub supports_char_literal: bool,
    pub supports_lifetime: bool,

    pub string_delimiters: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy, Debug)]
pub struct TsConfig {
    pub language: fn() -> tree_sitter::Language,
    pub highlights: &'static [&'static str],
    pub indent_kinds: &'static [&'static str],
}


#[derive(Debug)]
pub struct Language {
    pub config: &'static LanguageConfig,
    pub ts: Option<TsConfig>,
    pub indent_triggers: &'static [&'static str],
    pub multiline_strings: &'static [&'static str],
    pub case_insensitive: bool,
}

impl Language {
    pub const fn new(
        config: &'static LanguageConfig,
        indent_triggers: &'static [&'static str],
        multiline_strings: &'static [&'static str],
    ) -> Self {
        Self {
            config,
            ts: None,
            indent_triggers,
            multiline_strings,
            case_insensitive: false,
        }
    }

    pub const fn with_ts(mut self, ts: TsConfig) -> Self {
        self.ts = Some(ts);
        self
    }

    pub const fn case_insensitive(mut self) -> Self {
        self.case_insensitive = true;
        self
    }
}
