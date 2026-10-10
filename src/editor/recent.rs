use std::{
    env, fs,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 10;

fn data_file() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })?;
    Some(base.join("vmacs").join("recent"))
}

fn load() -> Vec<String> {
    let Some(file) = data_file() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn load_existing() -> Vec<String> {
    load()
        .into_iter()
        .filter(|path| Path::new(path).is_file())
        .collect()
}

pub fn record(path: &str) {
    let absolute = fs::canonicalize(path).or_else(|_| env::current_dir().map(|dir| dir.join(path)));
    let Ok(absolute) = absolute else {
        return;
    };
    let Some(absolute) = absolute.to_str().map(str::to_string) else {
        return;
    };
    let mut entries: Vec<String> = load().into_iter().filter(|p| *p != absolute).collect();
    entries.insert(0, absolute);
    entries.truncate(MAX_ENTRIES);

    let Some(file) = data_file() else {
        return;
    };
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(file, entries.join("\n"));
}