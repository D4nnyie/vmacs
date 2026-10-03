//! User-defined languages, loaded once from `<config>/vmacs/languages/*.toml`.
//!
//! Config dir: $XDG_CONFIG_HOME, else %APPDATA%, else $HOME/.config.
//! Files that fail to parse are skipped silently.

use super::{Language, LanguageConfig};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct PluginDef {
    name: String,
    extensions: Vec<String>,
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

fn leak_str(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn leak_strs(values: Vec<String>) -> &'static [&'static str] {
    let leaked: Vec<&'static str> = values.into_iter().map(leak_str).collect();
    Box::leak(leaked.into_boxed_slice())
}

fn leak_pairs(values: Vec<(String, String)>) -> &'static [(&'static str, &'static str)] {
    let leaked: Vec<(&'static str, &'static str)> = values
        .into_iter()
        .map(|(open, close)| (leak_str(open), leak_str(close)))
        .collect();
    Box::leak(leaked.into_boxed_slice())
}

fn config_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("vmacs").join("languages"))
}

fn load_one(path: &Path) -> Option<&'static Language> {
    let text = fs::read_to_string(path).ok()?;
    let def: PluginDef = ::toml::from_str(&text).ok()?;
    if def.extensions.is_empty() {
        return None;
    }

    let config: &'static LanguageConfig = Box::leak(Box::new(LanguageConfig {
        name: leak_str(def.name),
        extensions: leak_strs(def.extensions),
        keywords: leak_strs(def.keywords),
        types: leak_strs(def.types),
        known_values: leak_strs(def.known_values),
        line_comment: def.line_comment.map(leak_str),
        block_comment: def
            .block_comment
            .map(|(open, close)| (leak_str(open), leak_str(close))),
        supports_char_literal: def.supports_char_literal,
        supports_lifetime: def.supports_lifetime,
        string_delimiters: leak_pairs(def.string_delimiters),
    }));

    let mut language = Language::new(
        config,
        leak_strs(def.indent_triggers),
        leak_strs(def.multiline_strings),
    );
    if def.case_insensitive {
        language = language.case_insensitive();
    }
    Some(Box::leak(Box::new(language)))
}

pub fn load_all() -> Vec<&'static Language> {
    let Some(dir) = config_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    paths.iter().filter_map(|path| load_one(path)).collect()
}
