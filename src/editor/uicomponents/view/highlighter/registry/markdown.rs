use super::LanguageConfig;

pub const MARKDOWN: LanguageConfig = LanguageConfig {

    name: "Markdown",

    extensions: &["md", "markdown", "mdown", "mkdn"],

    keywords: &[],

    types: &[],

    known_values: &[],

    line_comment: None,

    block_comment: None,

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[],

};