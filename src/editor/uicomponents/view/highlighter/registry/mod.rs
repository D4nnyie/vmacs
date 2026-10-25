use super::language::{Language, LanguageConfig, LanguageSpec, TsConfig};
use std::sync::{Arc, OnceLock, RwLock};

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
mod extra;

pub use plugin::new_language_file;

const BRACES: &[&str] = &["{", "(", "["];

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn builtin() -> &'static [Arc<Language>] {
    static BUILTIN: OnceLock<Vec<Arc<Language>>> = OnceLock::new();
    BUILTIN.get_or_init(|| {
        vec![
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
            Language::new(&extra::JSON, &["{", "["], &[]),
            Language::new(&extra::CSS, &["{"], &[]),
            Language::new(&extra::MAKEFILE, &[], &[])
                .with_filenames(names(&["Makefile", "makefile", "GNUmakefile"])),
            Language::new(&extra::DOCKERFILE, &[], &[])
                .with_filenames(names(&["Dockerfile", "Containerfile"]))
                .case_insensitive(),
            Language::new(&extra::RUBY, &["do", "then", "{", "(", "["], &[])
                .with_filenames(names(&["Rakefile", "Gemfile"])),
            Language::new(&extra::KOTLIN, BRACES, &["\"\"\""]),
            Language::new(&extra::INI, &[], &[]),
        ]
        .into_iter()
        .map(Arc::new)
        .collect()
    })
}

static PLUGINS: RwLock<Option<Vec<Arc<Language>>>> = RwLock::new(None);

pub fn reload_plugins() -> (usize, Vec<String>) {
    let (languages, errors) = plugin::load_all();
    let count = languages.len();
    if let Ok(mut guard) = PLUGINS.write() {
        // The previous Vec is dropped here; its languages are freed once
        // no buffer or highlighter still holds a clone.
        *guard = Some(languages);
    }
    (count, errors)
}

fn plugins() -> Vec<Arc<Language>> {
    if let Ok(guard) = PLUGINS.read() {
        if let Some(languages) = guard.as_ref() {
            return languages.clone();
        }
    }
    reload_plugins();
    PLUGINS
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_default()
}

fn has_extension(language: &Language, ext: &str) -> bool {
    language
        .config
        .extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case(ext))
}

pub fn find_language(file_name: &str, ext: &str) -> Option<Arc<Language>> {
    let plugins = plugins();
    plugins
        .iter()
        .find(|lang| lang.filenames.iter().any(|name| name == file_name))
        .or_else(|| plugins.iter().find(|lang| has_extension(lang, ext)))
        .or_else(|| builtin().iter().find(|lang| has_extension(lang, ext)))
        .or_else(|| plugins.iter().find(|lang| has_extension(lang, ext)))
        .or_else(|| builtin().iter().find(|lang| lang.filenames.iter().any(|name| name == file_name)))
        .or_else(|| builtin().iter().find(|lang| has_extension(lang, ext)))
        .cloned()
}
