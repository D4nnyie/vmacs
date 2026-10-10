use serde::Deserialize;
use std::{collections::HashMap, env, fs, path::{Path, PathBuf}};
use super::errors;


const MAX_CONFIG_BYTES: u64 = 128 * 1024;
const MAX_COMMANDS: usize = 256;
const MAX_COMMAND_LINES: usize = 32;
const MAX_COMMAND_LEN: usize = 1024;

#[allow(clippy::integer_division)]
pub fn read_limited(path: &Path, limit: u64) -> Result<String, String> {
    use std::io::Read;
    let too_large = || format!("file is larger than {} KiB", limit / 1024);
    let meta = fs::metadata(path).map_err(|err| errors::reason(&err))?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    if meta.len() > limit {
        return Err(too_large());
    }
    let file = fs::File::open(path).map_err(|err| errors::reason(&err))?;
    let mut text = String::new();
    file.take(limit.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|err| errors::reason(&err))?;
    if u64::try_from(text.len()).map_or(true, |len| len > limit) {
        return Err(too_large());
    }
    Ok(text)
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct MouseConfig {
    pub copy_on_select: bool,
    pub primary_selection: bool,
    pub middle_click_paste: bool,
    pub multi_click_ms: u64,
    pub drag_scroll_max: usize,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum CustomCommand {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct StatusBarConfig {
    pub clock_12h: bool,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct ScopeConfig {
    pub guide: bool,
    pub highlight: bool,
    pub brackets: bool,
}

impl CustomCommand {
    fn raw(&self) -> Vec<&str> {
        match self {
            Self::One(line) => vec![line.as_str()],
            Self::Many(lines) => lines.iter().map(String::as_str).collect(),
        }
    }

    pub fn describe(&self) -> String {
        self.raw().join(" ; ")
    }

    pub fn expand(&self, args: &str) -> Result<Vec<String>, String> {
        let parts = split_args(args);
        let mut out = Vec::new();
        for line in self.raw() {
            let line = line.trim().trim_start_matches(':').trim();
            if line.is_empty() {
                continue;
            }
            let (text, used) = fill(line, args, &parts)?;
            if used || args.is_empty() {
                out.push(text);
            } else {
                out.push(format!("{text} {args}"));
            }
        }
        Ok(out)
    }
}

fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in input.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

fn fill(line: &str, args: &str, parts: &[String]) -> Result<(String, bool), String> {
    let mut out = String::new();
    let mut used = false;
    let mut rest = line;
    loop {
        let Some(open) = rest.find('{') else { break };
        let after = &rest[open.saturating_add(1)..];
        let Some(close) = after.find('}') else { break };
        out.push_str(&rest[..open]);
        let inner = &after[..close];
        if inner.is_empty() || inner == "*" {
            out.push_str(args);
            used = true;
        } else if let Some(n) = inner.parse::<usize>().ok().filter(|n| *n >= 1) {
            let value = parts.get(n.saturating_sub(1)).ok_or_else(|| {
                format!("missing argument {n} (got {})", parts.len())
            })?;
            out.push_str(value);
            used = true;
        } else {
            out.push('{');
            out.push_str(inner);
            out.push('}');
        }
        rest = &after[close.saturating_add(1)..];
    }
    out.push_str(rest);
    Ok((out, used))
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            copy_on_select: false,
            primary_selection: true,
            middle_click_paste: true,
            multi_click_ms: 400,
            drag_scroll_max: 10,
        }
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct ClipboardConfig {
    pub osc52: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self { osc52: true }
    }
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub mouse: MouseConfig,
    pub clipboard: ClipboardConfig,
    pub statusbar: StatusBarConfig,
    pub commands: HashMap<String, CustomCommand>,
    pub scope: ScopeConfig,
}

const TEMPLATE: &str = r#"# vmacs settings. Run :reload after editing.

[mouse]
# Copy a mouse selection to the system clipboard when you release the button.
copy_on_select = false
# Selecting with the mouse sets the PRIMARY selection (Linux), so middle click
# pastes it here and in other apps.
primary_selection = true
# Middle click pastes the PRIMARY selection (the normal clipboard off Linux). (due to some reasons it also copies text when selected, two birds with one stone.)
middle_click_paste = true
# Max time between clicks for a double / triple click in ms (0 disables both).
multi_click_ms = 400
# Fastest autoscroll (columns or lines per tick) when dragging past an edge.
drag_scroll_max = 10

[clipboard]
# Also send copies as an OSC 52 escape, so they reach the clipboard over SSH,
# in tmux, or where the native clipboard fails.
osc52 = true

[statusbar]
# Show the clock as 12-hour time (3:45 PM) instead of 24-hour (15:45).
clock_12h = false

[scope]
# A line beside the line numbers spanning the {}, () or [] pair around the cursor.
# It grows and shrinks as the cursor moves in and out of pairs.
guide = false
# Tint the text inside that pair.
highlight = false
# Highlight just the two bracket characters of that pair.
brackets = true

[commands]
write = "w"                    # alias for :w
quit = "q"                     # alias for :q
line-nu = "nu"                 # alias for :nu
force-quit = "q!"              # alias for :q!
save-quit = "wq"               # alias for :wq
file-manager = "e"             # alias for :e
file-manager-force = "e!"      # alias for :e!
save-quiet = ["w", "noh"]      # runs :w, then :noh
save-reload = ["w", "reload"]  # save and reload
trim = "%s/ +$//g"             # no {} -> typed args are appended
swap = "%s/{}/x/g"             # {} is all typed args
replace = "%s/{1}/{2}/g"       # \replace true false -> :%s/true/false/g
home = "dash"                  # :dashboard / :dash alias
"#;

pub fn path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("vmacs").join("config.toml"))
}

pub fn ensure_file() -> Result<PathBuf, String> {
    let path = path()
        .ok_or("ERR: no config directory (set HOME, XDG_CONFIG_HOME or APPDATA)")?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|err| {
                errors::describe("cannot create config directory", &dir.display().to_string(), &err)
            })?;
        }
        fs::write(&path, TEMPLATE).map_err(|err| {
            errors::describe("cannot write config file", &path.display().to_string(), &err)
        })?;
    }
    Ok(path)
}

impl Config {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = path() else {
            return (Self::default(), None);
        };
        if !path.exists() {
            return (Self::default(), None);
        }
        let text = match read_limited(&path, MAX_CONFIG_BYTES) {
            Ok(text) => text,
            Err(reason) => {
                return (Self::default(), Some(format!("ERR: config.toml: {reason}")));
            }
        };
        match ::toml::from_str::<Self>(&text) {
            Ok(config) => match config.check_limits() {
                Ok(()) => (config, None),
                Err(message) => (Self::default(), Some(format!("ERR: config.toml: {message}"))),
            },
            Err(err) => (
                Self::default(),
                Some(format!("ERR: config.toml: {}", errors::toml_detail(&text, &err))),
            ),
        }
    }

    fn check_limits(&self) -> Result<(), String> {
        if self.commands.len() > MAX_COMMANDS {
            return Err(format!("too many custom commands (limit {MAX_COMMANDS})"));
        }
        for (name, command) in &self.commands {
            let lines = command.raw();
            if lines.len() > MAX_COMMAND_LINES
                || lines.iter().any(|line| line.len() > MAX_COMMAND_LEN)
            {
                return Err(format!(
                    "command `{name}`: at most {MAX_COMMAND_LINES} lines of {MAX_COMMAND_LEN} bytes"
                ));
            }
        }
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn one(text: &str) -> CustomCommand {
        CustomCommand::One(text.to_string())
    }

    #[test]
    fn template_parses() {
        let config: Config = ::toml::from_str(TEMPLATE).expect("TEMPLATE must be valid");
        assert!(config.commands.contains_key("save-quit"));
        assert!(!config.statusbar.clock_12h);
        assert!(!config.scope.guide && !config.scope.highlight && config.scope.brackets);
        assert!(config.check_limits().is_ok());
    }

    #[test]
    fn scope_features_default_to_off() {
        let config = Config::default();
        assert!(!config.scope.guide && !config.scope.highlight && !config.scope.brackets);
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let config: Config = ::toml::from_str("[mouse]\nmulti_click_ms = 123\n").unwrap();
        assert_eq!(config.mouse.multi_click_ms, 123);
        assert!(config.mouse.middle_click_paste);
        assert!(config.clipboard.osc52);
    }

    #[test]
    fn split_args_handles_quotes() {
        assert_eq!(split_args("a b  c"), vec!["a", "b", "c"]);
        assert_eq!(split_args("\"foo bar\" baz"), vec!["foo bar", "baz"]);
        assert_eq!(split_args("'x y'"), vec!["x y"]);
        assert_eq!(split_args("\"\""), vec![""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn positional_arguments() {
        assert_eq!(
            one("%s/{1}/{2}/g").expand("true false"),
            Ok(vec!["%s/true/false/g".to_string()])
        );
        assert_eq!(
            one("%s/{2}/{1}/g").expand("a b"),
            Ok(vec!["%s/b/a/g".to_string()])
        );
    }

    #[test]
    fn missing_argument_is_an_error() {
        let err = one("%s/{1}/{2}/g").expand("only").unwrap_err();
        assert!(err.contains("missing argument 2"), "{err}");
    }

    #[test]
    fn whole_argument_string_still_works() {
        assert_eq!(one("%s/{}/x/g").expand("true false"), Ok(vec!["%s/true false/x/g".to_string()]));
        assert_eq!(one("%s/{*}/x/g").expand("a b"), Ok(vec!["%s/a b/x/g".to_string()]));
    }

    #[test]
    fn args_are_appended_without_placeholder() {
        assert_eq!(one("trim").expand("x"), Ok(vec!["trim x".to_string()]));
        assert_eq!(one("trim").expand(""), Ok(vec!["trim".to_string()]));
    }

    #[test]
    fn unknown_braces_are_kept_and_colon_is_trimmed() {
        assert_eq!(one(":echo {abc}").expand("x"), Ok(vec!["echo {abc} x".to_string()]));
    }

    #[test]
    fn command_lists_expand_in_order() {
        let command = CustomCommand::Many(vec!["w".to_string(), "noh".to_string()]);
        assert_eq!(command.expand(""), Ok(vec!["w".to_string(), "noh".to_string()]));
        assert_eq!(command.describe(), "w ; noh");
    }

    #[test]
    fn command_limits() {
        let mut config = Config::default();
        for n in 0..=MAX_COMMANDS {
            config.commands.insert(format!("c{n}"), one("w"));
        }
        assert!(config.check_limits().is_err());

        let mut config = Config::default();
        config.commands.insert("long".to_string(), one(&"x".repeat(MAX_COMMAND_LEN + 1)));
        assert!(config.check_limits().is_err());
    }

    #[test]
    fn read_limited_enforces_limits() {
        let dir = TempDir::new();
        let small = dir.write("small.toml", b"abc");
        assert_eq!(read_limited(&small, 10).as_deref(), Ok("abc"));

        let big = dir.write("big.toml", &[b'a'; 20]);
        assert!(read_limited(&big, 10).unwrap_err().contains("larger than"));

        assert!(read_limited(dir.path(), 10).unwrap_err().contains("not a regular file"));
        assert!(read_limited(&dir.path().join("missing"), 10).is_err());
    }
}