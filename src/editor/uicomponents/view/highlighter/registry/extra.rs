use super::LanguageConfig;

pub const JSON: LanguageConfig = LanguageConfig {
    name: "JSON",
    extensions: &["json", "jsonc", "geojson"],
    keywords: &[],
    types: &[],
    known_values: &["true", "false", "null"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\"")],
};

pub const CSS: LanguageConfig = LanguageConfig {
    name: "CSS",
    extensions: &["css", "scss", "less"],
    keywords: &["important", "media", "import", "supports", "keyframes", "from", "to", "and", "not", "only"],
    types: &[],
    known_values: &[
        "none", "auto", "inherit", "initial", "unset", "transparent", "solid", "flex", "grid",
        "block", "inline", "absolute", "relative", "fixed",
    ],
    line_comment: None,
    block_comment: Some(("/*", "*/")),
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("'", "'")],
};

pub const MAKEFILE: LanguageConfig = LanguageConfig {
    name: "Makefile",
    extensions: &["mk", "mak"],
    keywords: &[
        "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "include", "define", "endef",
        "export", "override", "vpath",
    ],
    types: &[],
    known_values: &[],
    line_comment: Some("#"),
    block_comment: None,
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("'", "'")],
};

pub const DOCKERFILE: LanguageConfig = LanguageConfig {
    name: "Dockerfile",
    extensions: &["dockerfile"],
    keywords: &[
        "FROM", "RUN", "CMD", "LABEL", "EXPOSE", "ENV", "ADD", "COPY", "ENTRYPOINT", "VOLUME",
        "USER", "WORKDIR", "ARG", "ONBUILD", "STOPSIGNAL", "HEALTHCHECK", "SHELL", "AS",
    ],
    types: &[],
    known_values: &[],
    line_comment: Some("#"),
    block_comment: None,
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("'", "'")],
};

pub const RUBY: LanguageConfig = LanguageConfig {
    name: "Ruby",
    extensions: &["rb", "rake", "gemspec"],
    keywords: &[
        "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
        "end", "ensure", "for", "if", "in", "module", "next", "not", "or", "redo", "rescue",
        "retry", "return", "self", "super", "then", "undef", "unless", "until", "when", "while",
        "yield", "require", "require_relative", "attr_accessor", "attr_reader", "attr_writer",
        "puts",
    ],
    types: &[],
    known_values: &["true", "false", "nil"],
    line_comment: Some("#"),
    block_comment: Some(("=begin", "=end")),
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("'", "'"), ("`", "`")],
};

pub const KOTLIN: LanguageConfig = LanguageConfig {
    name: "Kotlin",
    extensions: &["kt", "kts"],
    keywords: &[
        "as", "break", "class", "continue", "do", "else", "for", "fun", "if", "in", "interface",
        "is", "object", "package", "return", "super", "this", "throw", "try", "typealias", "val",
        "var", "when", "while", "by", "catch", "constructor", "finally", "init", "import",
        "override", "open", "abstract", "sealed", "data", "enum", "private", "protected",
        "public", "internal", "companion", "suspend", "inline", "lateinit", "const",
    ],
    types: &[
        "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "String", "Unit",
        "Any", "Nothing", "List", "Map", "Set", "Array",
    ],
    known_values: &["true", "false", "null"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    supports_char_literal: true,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("\"\"\"", "\"\"\"")],
};

pub const INI: LanguageConfig = LanguageConfig {
    name: "INI",
    extensions: &["ini", "cfg", "desktop"],
    keywords: &[],
    types: &[],
    known_values: &["true", "false", "yes", "no", "on", "off"],
    line_comment: Some(";"),
    block_comment: None,
    supports_char_literal: false,
    supports_lifetime: false,
    string_delimiters: &[("\"", "\""), ("'", "'")],
};