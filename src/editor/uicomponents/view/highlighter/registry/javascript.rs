use super::LanguageConfig;

pub const JAVASCRIPT: LanguageConfig = LanguageConfig {

    name: "JavaScript",

    extensions: &["js","mjs","cjs","jsx"],

    keywords: &[
        "as","async","await","break","case","catch","class","const","continue","debugger",
        "default","delete","do","else","export","extends","false","finally","for","from",
        "function","get","if","import","in","instanceof","let","new","null","of","return",
        "set","static","super","switch","this","throw","true","try","typeof","var","void",
        "while","with","yield",
    ],

    types: &[
        "Array","BigInt","Boolean","Date","Error","Function","Map","Number","Object","Promise",
        "RegExp","Set","String","Symbol","WeakMap","WeakSet",
    ],

    known_values: &["true","false","null","undefined","NaN","Infinity"],

    line_comment: Some("//"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
        ("`", "`"),
    ],

};