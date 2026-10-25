use super::LanguageConfig;

pub const HTML: LanguageConfig = LanguageConfig {

    name: "HTML",

    extensions: &["html", "htm"],

    keywords: &[
        "html","head","title","meta","link","style","script","body","header","footer",
        "main","nav","section","article","aside","div","span","p","a","img","picture",
        "source","video","audio","canvas","svg","form","input","textarea","button",
        "select","option","label","table","thead","tbody","tfoot","tr","th","td",
        "ul","ol","li","dl","dt","dd","h1","h2","h3","h4","h5","h6","br","hr",
    ],

    types: &[],

    known_values: &[],

    line_comment: None,

    block_comment: Some(("<!--", "-->")),

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
    ],

};