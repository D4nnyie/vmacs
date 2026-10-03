use super::LanguageConfig;

pub const TOML: LanguageConfig = LanguageConfig {

    name: "TOML",

    extensions: &["toml"],

    keywords: &[],

    types: &[
        "string","integer","float","boolean","datetime","array","table",
    ],

    known_values: &["true", "false"],

    line_comment: Some("#"),

    block_comment: None,

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
        ("\"\"\"", "\"\"\""),
        ("'''", "'''"),
    ],

};