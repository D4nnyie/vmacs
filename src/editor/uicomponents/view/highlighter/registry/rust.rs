use super::LanguageConfig;

pub const RUST: LanguageConfig = LanguageConfig {
    name: "Rust",
    extensions: &["rs"],
    keywords: &[
        "break","const","continue","crate","else","enum","extern","false","fn","for","if","impl",
        "in","let","loop","match","mod","move","mut","pub","ref","return","self","Self","static",
        "struct","super","trait","true","type","unsafe","use","where","while","async","await","dyn",
        "abstract","become","box","do","final","macro","override","priv","typeof","unsized","virtual",
        "yield","try","macro_rules","union",
    ],
    types: &[
        "i8","i16","i32","i64","i128","isize","u8","u16","u32","u64","u128","usize","f32","f64",
        "bool","char","Option","Result","String","str","Vec","HashMap",
    ],
    known_values: &["Some", "None", "true", "false", "Ok", "Err"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    supports_char_literal: true,
    supports_lifetime: true,

    string_delimiters: &[
        ("\"", "\""),
        ("r\"", "\""),
        ("r#\"", "\"#"),
    ],
};