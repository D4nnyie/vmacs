//! Tree-sitter grammars + queries for the built-in languages.
//!
//! If a `*_QUERY` constant below doesn't resolve, check the crate on docs.rs:
//! some grammar crates export `HIGHLIGHTS_QUERY`, others `HIGHLIGHT_QUERY`.
//! A language with a broken query silently falls back to the generic highlighter.

use super::TsConfig;

pub const RUST: TsConfig = TsConfig {
    language: || tree_sitter_rust::LANGUAGE.into(),
    highlights: &[tree_sitter_rust::HIGHLIGHTS_QUERY],
    indent_kinds: &[
        "block",
        "declaration_list",
        "field_declaration_list",
        "enum_variant_list",
        "match_block",
        "field_initializer_list",
        "use_list",
        "arguments",
        "parameters",
        "array_expression",
        "tuple_expression",
    ],
};

pub const PYTHON: TsConfig = TsConfig {
    language: || tree_sitter_python::LANGUAGE.into(),
    highlights: &[tree_sitter_python::HIGHLIGHTS_QUERY],
    indent_kinds: &[
        "argument_list",
        "parameters",
        "list",
        "dictionary",
        "tuple",
        "set",
        "parenthesized_expression",
    ],
};

pub const JAVASCRIPT: TsConfig = TsConfig {
    language: || tree_sitter_javascript::LANGUAGE.into(),
    highlights: &[
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
    ],
    indent_kinds: &[
        "statement_block",
        "class_body",
        "object",
        "object_pattern",
        "arguments",
        "formal_parameters",
        "array",
        "switch_body",
        "named_imports",
        "jsx_element",
        "parenthesized_expression",
    ],
};

pub const TYPESCRIPT: TsConfig = TsConfig {
    language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    highlights: &[
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
    ],
    indent_kinds: &[
        "statement_block",
        "class_body",
        "interface_body",
        "object_type",
        "enum_body",
        "object",
        "object_pattern",
        "arguments",
        "formal_parameters",
        "array",
        "switch_body",
        "named_imports",
        "parenthesized_expression",
    ],
};

pub const C: TsConfig = TsConfig {
    language: || tree_sitter_c::LANGUAGE.into(),
    highlights: &[tree_sitter_c::HIGHLIGHT_QUERY],
    indent_kinds: &[
        "compound_statement",
        "field_declaration_list",
        "enumerator_list",
        "initializer_list",
        "argument_list",
        "parameter_list",
        "parenthesized_expression",
    ],
};

pub const CPP: TsConfig = TsConfig {
    language: || tree_sitter_cpp::LANGUAGE.into(),
    highlights: &[
        tree_sitter_c::HIGHLIGHT_QUERY,
        tree_sitter_cpp::HIGHLIGHT_QUERY,
    ],
    indent_kinds: &[
        "compound_statement",
        "field_declaration_list",
        "declaration_list",
        "enumerator_list",
        "initializer_list",
        "argument_list",
        "parameter_list",
        "parenthesized_expression",
    ],
};

pub const GO: TsConfig = TsConfig {
    language: || tree_sitter_go::LANGUAGE.into(),
    highlights: &[tree_sitter_go::HIGHLIGHTS_QUERY],
    indent_kinds: &[
        "block",
        "field_declaration_list",
        "interface_type",
        "literal_value",
        "argument_list",
        "parameter_list",
        "import_spec_list",
    ],
};

pub const JAVA: TsConfig = TsConfig {
    language: || tree_sitter_java::LANGUAGE.into(),
    highlights: &[tree_sitter_java::HIGHLIGHTS_QUERY],
    indent_kinds: &[
        "block",
        "class_body",
        "interface_body",
        "enum_body",
        "constructor_body",
        "switch_block",
        "array_initializer",
        "argument_list",
        "formal_parameters",
    ],
};

pub const BASH: TsConfig = TsConfig {
    language: || tree_sitter_bash::LANGUAGE.into(),
    highlights: &[tree_sitter_bash::HIGHLIGHT_QUERY],
    indent_kinds: &[
        "do_group",
        "compound_statement",
        "subshell",
        "if_statement",
        "case_statement",
    ],
};
