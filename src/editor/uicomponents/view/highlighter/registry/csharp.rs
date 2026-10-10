use super::LanguageConfig;

pub const CSHARP: LanguageConfig = LanguageConfig {

    name: "C#",

    extensions: &["cs"],

    keywords: &[
        "abstract","as","base","bool","break","byte","case","catch","char","checked","class",
        "const","continue","decimal","default","delegate","do","double","else","enum","event",
        "explicit","extern","false","finally","fixed","float","for","foreach","goto","if",
        "implicit","in","int","interface","internal","is","lock","long","namespace","new",
        "null","object","operator","out","override","params","private","protected","public",
        "readonly","ref","return","sbyte","sealed","short","sizeof","stackalloc","static",
        "string","struct","switch","this","throw","true","try","typeof","uint","ulong","unchecked",
        "unsafe","ushort","using","virtual","void","volatile","while","async","await","get",
        "set","value","var","dynamic","nameof","record","init","required","file","global",
    ],

    types: &[
        "bool","byte","char","decimal","double","float","int","long","object","sbyte","short",
        "string","uint","ulong","ushort","void","dynamic",
    ],

    known_values: &["true","false","null"],

    line_comment: Some("//"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: true,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
    ],

};