//! User-defined languages from `<config>/vmacs/languages/*.toml`.
//!
//! Config dir: $XDG_CONFIG_HOME, else %APPDATA%, else $HOME/.config.
//! A file that fails to load is reported and skipped; the others still load.
//! Size and count limits keep a bad file from using unbounded memory or time.

use super::{Language, LanguageSpec};
use crate::editor::config::read_limited;
use crate::editor::errors;
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_FILE_BYTES: u64 = 128 * 1024;
const MAX_LANGUAGES: usize = 32;
const MAX_NAME_LEN: usize = 64;
const MAX_MATCHERS: usize = 32; // extensions and filenames
const MAX_MATCHER_LEN: usize = 64;
const MAX_WORDS: usize = 2000; // per keywords / types / known_values
const MAX_WORD_LEN: usize = 64;
const MAX_DELIMITERS: usize = 16; // string_delimiters and multiline_strings
const MAX_TRIGGERS: usize = 32;
const MAX_MARKER_LEN: usize = 16; // delimiters, comment markers, triggers

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginDef {
    #[serde(default)]
    name: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    filenames: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    known_values: Vec<String>,
    line_comment: Option<String>,
    block_comment: Option<(String, String)>,
    #[serde(default)]
    string_delimiters: Vec<(String, String)>,
    #[serde(default)]
    multiline_strings: Vec<String>,
    #[serde(default)]
    indent_triggers: Vec<String>,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default)]
    supports_char_literal: bool,
    #[serde(default)]
    supports_lifetime: bool,
}

fn config_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("vmacs").join("languages"))
}

fn check_list(what: &str, items: &[String], max_items: usize, max_len: usize) -> Result<(), String> {
    if items.len() > max_items {
        return Err(format!("`{what}` has {} entries (limit {max_items})", items.len()));
    }
    if items.iter().any(|item| item.is_empty() || item.len() > max_len) {
        return Err(format!("`{what}` entries must be 1-{max_len} bytes long"));
    }
    Ok(())
}

/// A delimiter, comment marker or indent trigger.
fn check_marker(what: &str, text: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > MAX_MARKER_LEN || text.chars().any(char::is_control) {
        return Err(format!(
            "`{what}`: \"{}\" must be 1-{MAX_MARKER_LEN} bytes with no control characters",
            text.escape_debug()
        ));
    }
    Ok(())
}

fn validate(def: &PluginDef) -> Result<(), String> {
    if def.name.len() > MAX_NAME_LEN {
        return Err(format!("`name` is longer than {MAX_NAME_LEN} bytes"));
    }
    check_list("filenames", &def.filenames, MAX_MATCHERS, MAX_MATCHER_LEN)?;
    check_list("keywords", &def.keywords, MAX_WORDS, MAX_WORD_LEN)?;
    check_list("types", &def.types, MAX_WORDS, MAX_WORD_LEN)?;
    check_list("known_values", &def.known_values, MAX_WORDS, MAX_WORD_LEN)?;

    if let Some(marker) = &def.line_comment {
        check_marker("line_comment", marker)?;
    }
    if let Some((open, close)) = &def.block_comment {
        check_marker("block_comment", open)?;
        check_marker("block_comment", close)?;
    }

    if def.string_delimiters.len() > MAX_DELIMITERS {
        return Err(format!("`string_delimiters` has more than {MAX_DELIMITERS} entries"));
    }
    for (open, close) in &def.string_delimiters {
        check_marker("string_delimiters", open)?;
        check_marker("string_delimiters", close)?;
    }

    if def.multiline_strings.len() > MAX_DELIMITERS {
        return Err(format!("`multiline_strings` has more than {MAX_DELIMITERS} entries"));
    }
    for item in &def.multiline_strings {
        check_marker("multiline_strings", item)?;
        if !def.string_delimiters.iter().any(|(open, _)| open == item) {
            return Err(format!(
                "`multiline_strings`: \"{}\" is not an opening delimiter in `string_delimiters`",
                item.escape_debug()
            ));
        }
    }

    if def.indent_triggers.len() > MAX_TRIGGERS {
        return Err(format!("`indent_triggers` has more than {MAX_TRIGGERS} entries"));
    }
    for trigger in &def.indent_triggers {
        check_marker("indent_triggers", trigger)?;
    }
    Ok(())
}

fn load_one(path: &Path) -> Result<Arc<Language>, String> {
    let text = read_limited(path, MAX_FILE_BYTES)?;
    let def: PluginDef = ::toml::from_str(&text).map_err(|err| errors::toml_detail(&text, &err))?;
    validate(&def)?;

    let extensions: Vec<String> = def
        .extensions
        .into_iter()
        .map(|ext| ext.trim().trim_start_matches('.').to_string())
        .filter(|ext| !ext.is_empty())
        .collect();
    check_list("extensions", &extensions, MAX_MATCHERS, MAX_MATCHER_LEN)?;
    if extensions.is_empty() && def.filenames.is_empty() {
        return Err("needs `extensions` or `filenames`, e.g. extensions = [\"abc\"]".to_string());
    }
    let name = if def.name.trim().is_empty() {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Custom")
            .chars()
            .take(MAX_NAME_LEN)
            .collect()
    } else {
        def.name
    };

    let spec = LanguageSpec {
        name,
        extensions,
        keywords: def.keywords,
        types: def.types,
        known_values: def.known_values,
        line_comment: def.line_comment,
        block_comment: def.block_comment,
        supports_char_literal: def.supports_char_literal,
        supports_lifetime: def.supports_lifetime,
        string_delimiters: def.string_delimiters,
    };

    let mut language = Language::from_parts(spec, def.indent_triggers, def.multiline_strings)
        .with_filenames(def.filenames);
    if def.case_insensitive {
        language = language.case_insensitive();
    }
    Ok(Arc::new(language))
}

pub fn load_all() -> (Vec<Arc<Language>>, Vec<String>) {
    let mut languages = Vec::new();
    let mut errors = Vec::new();
    let Some(dir) = config_dir() else {
        return (languages, errors);
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return (languages, errors);
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .take(MAX_LANGUAGES.saturating_add(1))
        .collect();
    paths.sort();
    if paths.len() > MAX_LANGUAGES {
        errors.push(format!("too many language files, only {MAX_LANGUAGES} are loaded"));
        paths.truncate(MAX_LANGUAGES);
    }
    for path in &paths {
        match load_one(path) {
            Ok(language) => languages.push(language),
            Err(message) => {
                let file = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?");
                errors.push(format!("{file}: {message}"));
            }
        }
    }
    (languages, errors)
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn def(text: &str) -> PluginDef {
        ::toml::from_str(text).expect("valid toml")
    }

    #[test]
    fn valid_definition_passes() {
        let d = def("extensions = [\"abc\"]\nstring_delimiters = [[\"'''\", \"'''\"]]\nmultiline_strings = [\"'''\"]\n");
        assert!(validate(&d).is_ok());
    }

    #[test]
    fn empty_and_oversized_markers_are_rejected() {
        assert!(validate(&def("string_delimiters = [[\"\", \"\"]]")).is_err());
        assert!(validate(&def("line_comment = \"\"")).is_err());
        assert!(validate(&def("line_comment = \"\\u0007\"")).is_err());
        let long = "x".repeat(MAX_MARKER_LEN + 1);
        assert!(validate(&def(&format!("line_comment = \"{long}\""))).is_err());
        assert!(validate(&def("block_comment = [\"/*\", \"\"]")).is_err());
        assert!(validate(&def("indent_triggers = [\"\"]")).is_err());
    }

    #[test]
    fn multiline_must_be_an_opening_delimiter() {
        let err = validate(&def("multiline_strings = [\"'''\"]")).unwrap_err();
        assert!(err.contains("multiline_strings"), "{err}");
    }

    #[test]
    fn list_limits() {
        let many: Vec<String> = (0..=MAX_WORDS).map(|n| format!("\"k{n}\"")).collect();
        let text = format!("keywords = [{}]", many.join(","));
        assert!(validate(&def(&text)).is_err());
        assert!(validate(&def("keywords = [\"\"]")).is_err());
        let long = "x".repeat(MAX_WORD_LEN + 1);
        assert!(validate(&def(&format!("types = [\"{long}\"]"))).is_err());
    }

    #[test]
    fn loads_a_language_file() {
        let dir = TempDir::new();
        let path = dir.write(
            "mylang.toml",
            b"extensions = [\".abc\"]\nkeywords = [\"Foo\"]\ncase_insensitive = true\n",
        );
        let lang = load_one(&path).unwrap();
        assert_eq!(lang.config.name, "mylang");
        assert_eq!(lang.config.extensions, vec!["abc".to_string()]);
        assert!(lang.case_insensitive);
        assert!(lang.words.keywords.contains("foo"));
    }

    #[test]
    fn rejects_bad_files() {
        let dir = TempDir::new();
        let no_match = dir.write("a.toml", b"keywords = [\"x\"]\n");
        assert!(load_one(&no_match).unwrap_err().contains("needs `extensions`"));

        let unknown = dir.write("b.toml", b"extensions = [\"x\"]\nbogus = 1\n");
        assert!(load_one(&unknown).is_err());

        let huge = dir.write("c.toml", &vec![b'#'; (MAX_FILE_BYTES + 1) as usize]);
        assert!(load_one(&huge).unwrap_err().contains("KiB"));
    }
}