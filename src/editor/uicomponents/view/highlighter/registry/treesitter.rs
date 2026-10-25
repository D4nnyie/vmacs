use super::TsConfig;

pub const RUST: TsConfig = TsConfig {
    language: || tree_sitter_rust::LANGUAGE.into(),
    highlights: &[
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        "(attribute_item) @attribute\n(inner_attribute_item) @attribute\n(mod_item name: (identifier) @module)\n\
         (scoped_identifier path: (identifier) @module)\n\
         (scoped_identifier path: (scoped_identifier name: (identifier) @module))\n\
         (scoped_use_list path: (identifier) @module)\n\
         (scoped_use_list path: (scoped_identifier name: (identifier) @module))",
    ],
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
    fold_kinds: &[
        "function_item", "impl_item", "trait_item", "struct_item", "enum_item", "mod_item",
        "macro_definition", "block", "declaration_list", "field_declaration_list",
        "enum_variant_list", "match_block", "field_initializer_list", "use_list", "block_comment",
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
    fold_kinds: &[
        "function_definition", "class_definition", "if_statement", "elif_clause", "else_clause",
        "for_statement", "while_statement", "try_statement", "except_clause", "finally_clause",
        "with_statement", "match_statement", "case_clause", "list", "dictionary", "set", "tuple",
        "argument_list", "parameters", "string",
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
    fold_kinds: &[
        "function_declaration", "class_declaration", "method_definition", "statement_block",
        "class_body", "object", "array", "switch_body", "arguments", "formal_parameters",
        "jsx_element", "template_string", "comment",
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
    fold_kinds: &[
        "function_declaration", "class_declaration", "method_definition", "statement_block",
        "class_body", "interface_body", "object_type", "enum_body", "object", "array",
        "switch_body", "arguments", "formal_parameters", "named_imports", "template_string",
        "comment",
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
    fold_kinds: &[
        "function_definition", "compound_statement", "field_declaration_list", "enumerator_list",
        "initializer_list", "preproc_if", "preproc_ifdef", "comment",
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
    fold_kinds: &[
        "function_definition", "compound_statement", "field_declaration_list", "declaration_list",
        "enumerator_list", "initializer_list", "preproc_if", "preproc_ifdef", "comment",
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
    fold_kinds: &[
        "function_declaration", "method_declaration", "block", "field_declaration_list",
        "interface_type", "literal_value", "import_declaration", "const_declaration",
        "var_declaration", "type_declaration", "comment",
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
    fold_kinds: &[
        "method_declaration", "constructor_declaration", "class_declaration",
        "interface_declaration", "enum_declaration", "block", "class_body", "interface_body",
        "enum_body", "switch_block", "array_initializer", "block_comment",
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
    fold_kinds: &[
        "function_definition", "do_group", "compound_statement", "if_statement", "case_statement",
        "subshell", "heredoc_body", "for_statement", "while_statement",
    ],
};