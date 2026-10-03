use super::LanguageConfig;

pub const YAML: LanguageConfig = LanguageConfig {

    name: "YAML",

    extensions: &["yaml", "yml"],

    keywords: &[],

    types: &[
        "null","boolean","integer","float","string","sequence","mapping",
    ],

    known_values: &[
        "true","false","null","True","False","Null","TRUE","FALSE","NULL",
        "yes","no","Yes","No","YES","NO",
    ],

    line_comment: Some("#"),

    block_comment: None,

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
    ],

};