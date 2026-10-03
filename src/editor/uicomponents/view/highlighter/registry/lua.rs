use super::LanguageConfig;

pub const LUA: LanguageConfig = LanguageConfig {

    name: "Lua",

    extensions: &["lua"],

    keywords: &[
        "and","break","do","else","elseif","end","false","for","function","goto","if","in",
        "local","nil","not","or","repeat","return","then","true","until","while",
    ],

    types: &[
        "nil","boolean","number","string","table","function","thread","userdata",
    ],

    known_values: &["true", "false", "nil"],

    line_comment: Some("--"),

    block_comment: Some(("--[[", "]]")),

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
        ("[[", "]]"),
    ],

};