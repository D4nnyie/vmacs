use super::LanguageConfig;

pub const PYTHON: LanguageConfig = LanguageConfig {
    name: "Python",
    extensions: &["py", "pyw"],
    keywords: &[
        "False","None","True","and","as","assert","async","await","break","class","continue","def",
        "del","elif","else","except","finally","for","from","global","if","import","in","is","lambda",
        "nonlocal","not","or","pass","raise","return","try","while","with","yield",
    ],
    types: &[
        "int","float","str","bool","list","dict","tuple","set","frozenset","bytes","bytearray",
        "complex","object",
    ],
    known_values: &["True", "False", "None"],
    line_comment: Some("#"),
    block_comment: None,
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[
        ("\"", "\""),
        ("'", "'"),
        ("\"\"\"", "\"\"\""),
        ("'''", "'''"),
    ],
};