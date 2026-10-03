use super::LanguageConfig;

pub const GO: LanguageConfig = LanguageConfig {

    name: "Go",

    extensions: &["go"],

    keywords: &[
        "break","default","func","interface","select","case","defer","go","map","struct",
        "chan","else","goto","package","switch","const","fallthrough","if","range","type",
        "continue","for","import","return","var",
    ],

    types: &[
        "bool","byte","complex64","complex128","error","float32","float64","int","int8",
        "int16","int32","int64","rune","string","uint","uint8","uint16","uint32","uint64",
        "uintp","uintptr",
    ],

    known_values: &["true", "false", "nil", "iota"],

    line_comment: Some("//"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: true,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("`", "`"),
    ],

};