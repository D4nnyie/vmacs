use std::{
    fmt::{self, Display},
    path::{Path, PathBuf},
};

use super::highlighter::{find_by_extension, Language};

#[derive(Default, Debug)]
pub struct FileInfo {
    path: Option<PathBuf>,
    language: Option<&'static Language>,
}

impl FileInfo {
    pub fn from(file_name: &str) -> Self {
        let path = PathBuf::from(file_name);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let language = find_by_extension(extension);
        Self { path: Some(path), language }
    }
    pub fn get_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub const fn has_path(&self) -> bool {
        self.path.is_some()
    }
    pub const fn get_language(&self) -> Option<&'static Language> {
        self.language
    }
    pub fn file_type_name(&self) -> &'static str {
        self.language.map_or("Text", |lang| lang.config.name)
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
