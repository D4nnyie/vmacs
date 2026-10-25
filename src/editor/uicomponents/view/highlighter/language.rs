use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct WordSets {
    pub keywords: HashSet<String>,
    pub types: HashSet<String>,
    pub known_values: HashSet<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct LanguageConfig {
    pub name: &'static str,
    pub extensions: &'static [&'static str],

    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub known_values: &'static [&'static str],

    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,

    pub supports_char_literal: bool,
    pub supports_lifetime: bool,

    pub string_delimiters: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy, Debug)]
pub struct TsConfig {
    pub language: fn() -> tree_sitter::Language,
    pub highlights: &'static [&'static str],
    pub indent_kinds: &'static [&'static str],
    pub fold_kinds: &'static [&'static str],
}


#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LanguageSpec {
    pub name: String,
    pub extensions: Vec<String>,
    pub keywords: Vec<String>,
    pub types: Vec<String>,
    pub known_values: Vec<String>,
    pub line_comment: Option<String>,
    pub block_comment: Option<(String, String)>,
    pub supports_char_literal: bool,
    pub supports_lifetime: bool,
    pub string_delimiters: Vec<(String, String)>,
}

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

impl From<&LanguageConfig> for LanguageSpec {
    fn from(config: &LanguageConfig) -> Self {
        Self {
            name: config.name.to_string(),
            extensions: owned(config.extensions),
            keywords: owned(config.keywords),
            types: owned(config.types),
            known_values: owned(config.known_values),
            line_comment: config.line_comment.map(str::to_string),
            block_comment: config
                .block_comment
                .map(|(open, close)| (open.to_string(), close.to_string())),
            supports_char_literal: config.supports_char_literal,
            supports_lifetime: config.supports_lifetime,
            string_delimiters: config
                .string_delimiters
                .iter()
                .map(|(open, close)| ((*open).to_string(), (*close).to_string()))
                .collect(),
        }
    }
}

fn word_set(words: &[String], case_insensitive: bool) -> HashSet<String> {
    words
        .iter()
        .map(|word| {
            if case_insensitive {
                word.to_ascii_lowercase()
            } else {
                word.clone()
            }
        })
        .collect()
}

#[derive(Debug)]
pub struct Language {
    pub config: LanguageSpec,
    pub ts: Option<TsConfig>,
    pub indent_triggers: Vec<String>,
    pub multiline_strings: Vec<String>,
    pub filenames: Vec<String>,
    pub case_insensitive: bool,
    pub words: WordSets,
}

impl Language {
    pub fn new(
        config: &LanguageConfig,
        indent_triggers: &[&str],
        multiline_strings: &[&str],
    ) -> Self {
        Self::from_parts(config.into(), owned(indent_triggers), owned(multiline_strings))
    }

    pub fn from_parts(
        config: LanguageSpec,
        indent_triggers: Vec<String>,
        multiline_strings: Vec<String>,
    ) -> Self {
        let mut language = Self {
            config,
            ts: None,
            indent_triggers,
            multiline_strings,
            filenames: Vec::new(),
            case_insensitive: false,
            words: WordSets::default(),
        };
        language.rebuild_words();
        language
    }

    fn rebuild_words(&mut self) {
        let ci = self.case_insensitive;
        self.words = WordSets {
            keywords: word_set(&self.config.keywords, ci),
            types: word_set(&self.config.types, ci),
            known_values: word_set(&self.config.known_values, ci),
        };
    }

    pub fn with_filenames(mut self, filenames: Vec<String>) -> Self {
        self.filenames = filenames;
        self
    }

    pub fn with_ts(mut self, ts: TsConfig) -> Self {
        self.ts = Some(ts);
        self
    }

    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = true;
        self.rebuild_words();
        self
    }
}
