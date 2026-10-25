use std::{
    env, fs,
    path::{Path, PathBuf},
};

use super::errors;

pub const MAX_PINS: usize = 20;

fn data_file() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })?;
    Some(base.join("vmacs").join("pinned"))
}

pub fn load() -> Vec<String> {
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

fn absolute(path: &str) -> Option<String> {
    fs::canonicalize(path)
        .or_else(|_| env::current_dir().map(|dir| dir.join(path)))
        .ok()?
        .to_str()
        .map(str::to_string)
}

fn save(pins: &[String]) -> Result<(), String> {
    let file = data_file().ok_or("no data directory (set HOME, XDG_DATA_HOME or APPDATA)")?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|err| errors::reason(&err))?;
    }
    fs::write(file, pins.join("\n")).map_err(|err| errors::reason(&err))
}

pub fn is_pinned(path: &str) -> bool {
    absolute(path).is_some_and(|abs| load().contains(&abs))
}

pub fn pin(path: &str) -> Result<bool, String> {
    let abs = absolute(path).ok_or("cannot resolve the path")?;
    if !Path::new(&abs).is_file() {
        return Err("not an existing file (save it first)".to_string());
    }
    let mut pins = load();
    if pins.contains(&abs) {
        return Ok(false);
    }
    if pins.len() >= MAX_PINS {
        return Err(format!("too many pins (limit {MAX_PINS}); unpin one first"));
    }
    pins.push(abs);
    save(&pins)?;
    Ok(true)
}

pub fn unpin(path: &str) -> Result<bool, String> {
    let abs = absolute(path).ok_or("cannot resolve the path")?;
    let mut pins = load();
    let before = pins.len();
    pins.retain(|pinned| *pinned != abs);
    if pins.len() == before {
        return Ok(false);
    }
    save(&pins)?;
    Ok(true)
}