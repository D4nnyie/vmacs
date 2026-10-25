use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
};

const MAX_ENTRIES: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ex,
    Custom,
}

#[derive(Default)]
pub struct CommandHistory {
    ex: Vec<String>,
    custom: Vec<String>,
}

fn data_file() -> Option<PathBuf> {
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })?;
    Some(base.join("vmacs").join("history"))
}

fn cap(list: &mut Vec<String>) {
    let excess = list.len().saturating_sub(MAX_ENTRIES);
    if excess > 0 {
        list.drain(..excess);
    }
}

fn canonical_ex(line: &str) -> String {
    let word = line.split_whitespace().next().unwrap_or("");
    let rest = word.trim_start_matches(|c: char| c.is_ascii_digit() || "%.$'<>,+-".contains(c));
    if rest.is_empty() {
        return if word.is_empty() { String::new() } else { "goto".to_string() };
    }
    if rest.starts_with('s') && rest.chars().nth(1).is_some_and(|c| !c.is_alphanumeric()) {
        return "s".to_string();
    }
    match rest {
        "write" => "w",
        "quit" => "q",
        "quit!" => "q!",
        "edit" => "e",
        "edit!" => "e!",
        "nu" => "number",
        "ap" => "autopairs",
        "u" => "undo",
        "red" => "redo",
        "dash" => "dashboard",
        "dash!" => "dashboard!",
        "h" => "help",
        "ver" => "version",
        "query-replace" => "qr",
        "nohlsearch" => "noh",
        "go" => "goto",
        other => other,
    }
    .to_string()
}

impl CommandHistory {
    pub fn load() -> Self {
        let mut history = Self::default();
        let Some(file) = data_file() else {
            return history;
        };
        let Ok(text) = fs::read_to_string(file) else {
            return history;
        };
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix(':') {
                history.ex.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix('\\') {
                history.custom.push(rest.to_string());
            }
        }
        cap(&mut history.ex);
        cap(&mut history.custom);
        history
    }

    fn list(&self, kind: Kind) -> &Vec<String> {
        match kind {
            Kind::Ex => &self.ex,
            Kind::Custom => &self.custom,
        }
    }

    pub fn record(&mut self, kind: Kind, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let list = match kind {
            Kind::Ex => &mut self.ex,
            Kind::Custom => &mut self.custom,
        };
        list.push(line.to_string());
        cap(list);
        self.save();
    }

    pub fn entries(&self, kind: Kind) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out: Vec<String> = self
            .list(kind)
            .iter()
            .rev()
            .filter(|line| seen.insert(line.as_str()))
            .cloned()
            .collect();
        out.reverse();
        out
    }

    pub fn usage(&self, kind: Kind) -> HashMap<String, usize> {
        let mut map: HashMap<String, usize> = HashMap::new();
        for line in self.list(kind) {
            let name = match kind {
                Kind::Ex => canonical_ex(line),
                Kind::Custom => line.split_whitespace().next().unwrap_or("").to_string(),
            };
            if !name.is_empty() {
                let count = map.entry(name).or_insert(0);
                *count = count.saturating_add(1);
            }
        }
        map
    }

    fn save(&self) {
        let Some(file) = data_file() else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut out = String::new();
        for line in &self.ex {
            out.push(':');
            out.push_str(line);
            out.push('\n');
        }
        for line in &self.custom {
            out.push('\\');
            out.push_str(line);
            out.push('\n');
        }
        let _ = fs::write(file, out);
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names() {
        assert_eq!(canonical_ex("w"), "w");
        assert_eq!(canonical_ex("write file.txt"), "w");
        assert_eq!(canonical_ex("nu"), "number");
        assert_eq!(canonical_ex("q!"), "q!");
        assert_eq!(canonical_ex("42"), "goto");
        assert_eq!(canonical_ex("%s/a/b/g"), "s");
        assert_eq!(canonical_ex("'<,'>s/a/b/"), "s");
        assert_eq!(canonical_ex("set"), "set");
        assert_eq!(canonical_ex(""), "");
    }

    #[test]
    fn entries_are_deduplicated_keeping_the_latest() {
        let history = CommandHistory {
            ex: vec!["w".into(), "q".into(), "w".into()],
            custom: Vec::new(),
        };
        assert_eq!(history.entries(Kind::Ex), vec!["q".to_string(), "w".to_string()]);
    }

    #[test]
    fn usage_counts_by_command_name() {
        let history = CommandHistory {
            ex: vec!["w".into(), "write x".into(), "42".into()],
            custom: vec!["swap a b".into(), "swap c d".into()],
        };
        let ex = history.usage(Kind::Ex);
        assert_eq!(ex.get("w"), Some(&2));
        assert_eq!(ex.get("goto"), Some(&1));
        assert_eq!(history.usage(Kind::Custom).get("swap"), Some(&2));
    }

    #[test]
    fn history_is_capped() {
        let mut list: Vec<String> = (0..MAX_ENTRIES + 10).map(|n| n.to_string()).collect();
        cap(&mut list);
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0], "10");
    }
}