use super::LanguageConfig;

pub const TYPESCRIPT: LanguageConfig = LanguageConfig {

    name: "TypeScript",

    extensions: &["ts","tsx","mts","cts"],

    keywords: &[
        "as","async","await","break","case","catch","class","const","continue","debugger",
        "declare","default","delete","do","else","enum","export","extends","false","finally",
        "for","from","function","get","if","implements","import","in","infer","instanceof",
        "interface","is","keyof","let","module","namespace","never","new","null","of","private",
        "protected","public","readonly","require","return","set","static","super","switch",
        "this","throw","true","try","type","typeof","undefined","unique","unknown","var","void",
        "while","with","yield",
    ],

    types: &[
        "any","bigint","boolean","never","number","object","string","symbol","unknown","void",
        "Array","Date","Map","Promise","Record","Set","String","Number","Boolean",
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