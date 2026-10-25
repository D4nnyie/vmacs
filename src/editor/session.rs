use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const MAX_FILES: usize = 50;

pub struct Entry {
    pub path: String,
    pub line: usize,
}

pub struct Session {
    pub files: Vec<Entry>,
    pub active: usize,
}

fn data_file() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })?;
    Some(base.join("vmacs").join("session"))
}

pub fn load() -> Option<Session> {
    let text = fs::read_to_string(data_file()?).ok()?;
    let mut active_raw = 0usize;
    let mut raw: Vec<Entry> = Vec::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("active\t") {
            active_raw = n.trim().parse().unwrap_or(0);
        } else if let Some((l, path)) = line.split_once('\t') {
            if let Ok(line_no) = l.trim().parse::<usize>() {
                raw.push(Entry { path: path.to_string(), line: line_no });
            }
        }
        if raw.len() >= MAX_FILES {
            break;
        }
    }
    let mut files = Vec::new();
    let mut active = 0usize;
    for (i, entry) in raw.into_iter().enumerate() {
        if Path::new(&entry.path).is_file() {
            if i == active_raw {
                active = files.len();
            }
            files.push(entry);
        }
    }
    (!files.is_empty()).then_some(Session { files, active })
}

pub fn count() -> usize {
    load().map_or(0, |session| session.files.len())
}

pub fn save(files: &[(String, usize)], active: usize) {
    if files.is_empty() {
        return;
    }
    let Some(file) = data_file() else {
        return;
    };
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut out = format!("active\t{active}\n");
    for (path, line) in files {
        out.push_str(&format!("{line}\t{path}\n"));
    }
    let _ = fs::write(file, out);
}