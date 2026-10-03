use super::LanguageConfig;

pub const BASH: LanguageConfig = LanguageConfig {
    name: "Bash",
    extensions: &["sh","bash"],

    keywords: &[
        "if","then","else","elif","fi","for","while","until","do","done","case","esac",
        "in","function","select","time","coproc","return","break","continue","source",
    ],

    types: &[],
    known_values: &["true","false"],
    line_comment: Some("#"),
    block_comment: None,
    supports_char_literal: false,
    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
    ],

};