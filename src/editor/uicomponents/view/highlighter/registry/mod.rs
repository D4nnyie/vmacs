use super::language::{Language, LanguageConfig, TsConfig};
use std::sync::OnceLock;

mod bash;
mod c;
mod cpp;
mod csharp;
mod go;
mod html;
mod java;
mod javascript;
mod lua;
mod markdown;
mod plugin;
mod python;
mod rust;
mod sql;
mod toml;
mod treesitter;
mod typescript;
mod yaml;

const BRACES: &[&str] = &["{", "(", "["];

static BUILTIN: &[Language] = &[
    Language::new(&rust::RUST, BRACES, &["\"", "r\"", "r#\""]).with_ts(treesitter::RUST),
    Language::new(&python::PYTHON, &[":", "(", "[", "{"], &["\"\"\"", "'''"])
        .with_ts(treesitter::PYTHON),
    Language::new(&cpp::CPP, BRACES, &["R\""]).with_ts(treesitter::CPP),
    Language::new(&bash::BASH, &["{", "(", "then", "do", "else"], &[]).with_ts(treesitter::BASH),
    Language::new(&csharp::CSHARP, BRACES, &[]),
    Language::new(&typescript::TYPESCRIPT, BRACES, &["`"]).with_ts(treesitter::TYPESCRIPT),
    Language::new(&javascript::JAVASCRIPT, BRACES, &["`"]).with_ts(treesitter::JAVASCRIPT),
    Language::new(&c::C, BRACES, &[]).with_ts(treesitter::C),
    Language::new(
        &lua::LUA,
        &["then", "do", "else", "repeat", "{", "(", "["],
        &["[["],
    ),
    Language::new(&java::JAVA, BRACES, &["\"\"\""]).with_ts(treesitter::JAVA),
    Language::new(&sql::SQL, &["("], &[]).case_insensitive(),
    Language::new(&markdown::MARKDOWN, &[], &[]),
    Language::new(&html::HTML, &[], &[]),
    Language::new(&go::GO, BRACES, &["`"]).with_ts(treesitter::GO),
    Language::new(&toml::TOML, &["[", "{"], &["\"\"\"", "'''"]),
    Language::new(&yaml::YAML, &[":"], &[]),
];

static PLUGINS: OnceLock<Vec<&'static Language>> = OnceLock::new();

fn has_extension(language: &Language, ext: &str) -> bool {
    language
        .config
        .extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case(ext))
}

/// Plugin languages (from `<config>/vmacs/languages/*.toml`) win over built-ins,
/// so a user can also override a built-in language's generic highlighting.
pub fn find_by_extension(ext: &str) -> Option<&'static Language> {
    let plugins = PLUGINS.get_or_init(plugin::load_all);
    plugins
        .iter()
        .copied()
        .find(|lang| has_extension(lang, ext))
        .or_else(|| BUILTIN.iter().find(|lang| has_extension(lang, ext)))
}
