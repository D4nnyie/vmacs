use super::LanguageConfig;

pub const SQL: LanguageConfig = LanguageConfig {

    name: "SQL",

    extensions: &["sql"],

    keywords: &[
        "SELECT","FROM","WHERE","INSERT","INTO","VALUES","UPDATE","SET","DELETE","CREATE",
        "ALTER","DROP","TABLE","DATABASE","INDEX","VIEW","TRIGGER","PROCEDURE","FUNCTION",
        "JOIN","INNER","LEFT","RIGHT","FULL","OUTER","CROSS","ON","AS","AND","OR","NOT",
        "NULL","IS","IN","BETWEEN","LIKE","ORDER","BY","GROUP","HAVING","LIMIT","OFFSET",
        "UNION","ALL","DISTINCT","PRIMARY","KEY","FOREIGN","REFERENCES","CONSTRAINT",
        "UNIQUE","CHECK","DEFAULT","CASCADE","CASE","WHEN","THEN","ELSE","END","BEGIN",
        "COMMIT","ROLLBACK","GRANT","REVOKE","WITH","RETURNING",
    ],

    types: &[
        "INTEGER","INT","BIGINT","SMALLINT","DECIMAL","NUMERIC","REAL","FLOAT","DOUBLE",
        "BOOLEAN","CHAR","VARCHAR","TEXT","DATE","TIME","TIMESTAMP","BLOB",
    ],

    known_values: &["TRUE", "FALSE", "NULL"],

    line_comment: Some("--"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: false,

    supports_lifetime: false,

    string_delimiters: &[
        ("'", "'"),
    ],

};