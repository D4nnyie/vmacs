use std::{
    fmt::{self, Display},
    path::{Path, PathBuf},
};
use std::sync::Arc;

use super::highlighter::{find_language, Language};

#[derive(Default, Debug)]
pub struct FileInfo {
    path: Option<PathBuf>,
    language: Option<Arc<Language>>,
}

impl FileInfo {
    fn detect(path: &Path) -> Option<Arc<Language>> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        find_language(name, ext)
    }
    pub fn from(file_name: &str) -> Self {
        let path = PathBuf::from(file_name);
        let language = Self::detect(&path);
        Self { path: Some(path), language }
    }
    pub fn refresh_language(&mut self) {
        self.language = self.path.as_deref().and_then(Self::detect);
    }
    pub fn get_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub const fn has_path(&self) -> bool {
        self.path.is_some()
    }
    pub fn get_language(&self) -> Option<&Arc<Language>> {
        self.language.as_ref()
    }
    pub fn file_type_name(&self) -> String {
        self.language
            .as_ref()
            .map_or_else(|| "Text".to_string(), |lang| lang.config.name.clone())
    }

    pub fn detect_shebang(&mut self, first_line: &str) {
        if self.language.is_some() {
            return;
        }
        let Some(rest) = first_line.strip_prefix("#!") else {
            return;
        };
        let ext = if rest.contains("python") {
            "py"
        } else if rest.contains("node") || rest.contains("deno") {
            "js"
        } else if rest.contains("ruby") {
            "rb"
        } else if rest.contains("lua") {
            "lua"
        } else if rest.contains("bash")
            || rest.contains("zsh")
            || rest.split_whitespace().last().is_some_and(|w| w == "sh" || w.ends_with("/sh"))
        {
            "sh"
        } else {
            return;
        };
        self.language = find_language("", ext);
    }
}

impl Display for FileInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self
            .get_path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("[No Name]");
        write!(formatter, "{name}")
    }
}