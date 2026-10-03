use super::LanguageConfig;

pub const C: LanguageConfig = LanguageConfig {
    name: "C",
    extensions: &["c", "h"],
    keywords: &[
        "auto","break","case","char","const","continue","default","do","double","else","enum",
        "extern","float","for","goto","if","inline","int","long","register","restrict","return",
        "short","signed","sizeof","static","struct","switch","typedef","union","unsigned","void",
        "volatile","while","_Alignas","_Alignof","_Atomic","_Bool","_Complex","_Generic",
        "_Imaginary","_Noreturn","_Static_assert","_Thread_local",
    ],

    types: &[
        "char","short","int","long","float","double","void","_Bool",
        "size_t","ptrdiff_t","int8_t","int16_t","int32_t","int64_t",
        "uint8_t","uint16_t","uint32_t","uint64_t",
    ],

    known_values: &["NULL","true","false"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    supports_char_literal: true,
    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
    ],

};