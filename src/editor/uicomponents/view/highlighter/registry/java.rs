use super::LanguageConfig;

pub const JAVA: LanguageConfig = LanguageConfig {

    name: "Java",

    extensions: &["java"],

    keywords: &[
        "abstract","assert","boolean","break","byte","case","catch","char","class","const",
        "continue","default","do","double","else","enum","extends","final","finally","float",
        "for","goto","if","implements","import","instanceof","int","interface","long","native",
        "new","package","private","protected","public","return","short","static","strictfp",
        "super","switch","synchronized","this","throw","throws","transient","try","void",
        "volatile","while","true","false","null","var","record","sealed","permits","non-sealed",
        "yield",
    ],

    types: &[
        "boolean","byte","char","double","float","int","long","short","void",
        "String","Object","Integer","Long","Double","Float","Boolean","Character",
        "Byte","Short",
    ],

    known_values: &["true", "false", "null"],

    line_comment: Some("//"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: true,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("\"\"\"", "\"\"\""),
    ],

};