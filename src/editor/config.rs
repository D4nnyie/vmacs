use serde::Deserialize;
use std::{collections::HashMap, env, fs, path::{Path, PathBuf}};
use super::errors;
use super::keymap::KeyMap;

const MAX_CONFIG_BYTES: u64 = 128 * 1024;
const MAX_COMMANDS: usize = 256;
const MAX_COMMAND_LINES: usize = 32;
const MAX_COMMAND_LEN: usize = 1024;
const MAX_KEYBINDS: usize = 256;

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

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct NotifyConfig {
    pub enabled: bool,
    pub max_visible: usize,
    pub timeout_ms: u64,
    pub big_edit_lines: usize,
}
impl Default for NotifyConfig {
    fn default() -> Self {
        Self { enabled: true, max_visible: 4, timeout_ms: 3000, big_edit_lines: 10 }
    }
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
    pub full_path: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct ScopeConfig {
    pub guide: bool,
    pub highlight: bool,
    pub brackets: bool,
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self { guide: true, highlight: false, brackets: false }
    }
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum UiMode {
    #[default]
    Workstation,
    Full,
    Compact,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct CompletionConfig {
    pub enabled: bool,
    pub auto: bool,
    pub min_chars: usize,
    pub max_items: usize,
    pub animate: bool,
}
impl Default for CompletionConfig {
    fn default() -> Self {
        Self { enabled: true, auto: true, min_chars: 2, max_items: 8, animate: true }
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct UiConfig {
    pub mode: UiMode,
    pub animations: bool,
    pub powerline: bool,
    pub prompt_animation: bool,
}
impl Default for UiConfig {
    fn default() -> Self {
        Self { mode: UiMode::Workstation, animations: true, powerline: false, prompt_animation: false }
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct YankConfig {
    pub enabled: bool,
    pub max_entries: usize,
    pub max_entry_bytes: usize,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct KeysConfig {
    pub all: HashMap<String, CustomCommand>,
    #[serde(alias = "normal")]
    pub control: HashMap<String, CustomCommand>,
    #[serde(alias = "insert")]
    pub input: HashMap<String, CustomCommand>,
    #[serde(alias = "visual")]
    pub mark: HashMap<String, CustomCommand>,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    #[default]
    Auto,
    On,
    Nerd,
    Off,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DashLayout {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct DashboardConfig {
    pub icons: IconMode,
    pub status_bar: bool,
    pub boxed: bool,
    pub layout: DashLayout,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self { icons: IconMode::Auto, status_bar: true, boxed: false, layout: DashLayout::Center }
    }
}

impl Default for YankConfig {
    fn default() -> Self {
        Self { enabled: false, max_entries: 20, max_entry_bytes: 4096 }
    }
}

#[derive(Deserialize, Clone, Copy)]
#[serde(default)]
pub struct TabsConfig {
    pub icons: IconMode,
    pub active_line: bool,
}

impl Default for TabsConfig {
    fn default() -> Self {
        Self { icons: IconMode::Auto, active_line: true }
    }
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundMode {
    #[default]
    Auto,
    Solid,
    Transparent,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct ThemeConfig {
    pub mode: BackgroundMode,
    pub background: String,
    pub foreground: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: BackgroundMode::Auto,
            background: "#161a22".to_string(),
            foreground: "#c5cfdb".to_string(),
        }
    }
}

pub fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let t = text.trim().trim_start_matches('#');
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some((n(0)?, n(2)?, n(4)?))
}

impl CustomCommand {
    fn raw(&self) -> Vec<&str> {
        match self {
            Self::One(line) => vec![line.as_str()],
            Self::Many(lines) => lines.iter().map(String::as_str).collect(),
        }
    }

    pub fn steps(&self) -> Vec<&str> {
        self.raw()
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
    pub yanks: YankConfig,
    pub keys: KeysConfig,
    pub dashboard: DashboardConfig,
    #[serde(skip)]
    pub keymap: KeyMap,
    pub tabs: TabsConfig,
    pub theme: ThemeConfig,
    pub ui: UiConfig,
    pub completion: CompletionConfig,
    pub notifications: NotifyConfig,
}

const TEMPLATE: &str = r##"# vmacs settings. Run :reload after editing.

[mouse]
# Copy a mouse selection to the system clipboard when you release the button.
copy_on_select = false
# Selecting with the mouse sets the PRIMARY selection (Linux), so middle click
# pastes it here and in other apps.
primary_selection = true
# Middle click pastes the PRIMARY selection (the normal clipboard of Linux). (due to some reasons it also copies text when selected.)
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
# Show the full path of the file instead of just its name (:path toggles).
full_path = false

[ui]
# "workstation": header + buffer bar + mode-first status bar, boxed file manager (default).
# "full": the previous interface. "compact": condensed. :ui cycles them for the session.
mode = "workstation"
# Soft Animations
animations = true
# Powerline arrows on bars (needs a patched font). Workstation looks clean without them.
powerline = false
# The centered : \ prompt box grows open. Needs animations = true.
prompt_animation = false

[scope]
# Indent guides: dim vertical lines at each indent level. The guide of the
# {}, () or [] block around the cursor lights up while you are inside it.
guide = true
# Tint the text inside that pair.
highlight = false
# Highlight just the two bracket characters of that pair.
brackets = true

[yanks]
# Keep a small in-memory history of copied text (yy, y, M-w, c, M-c).
# Off by default: copied text can contain passwords or tokens. It is never
# written to disk and is gone when you quit. :yanks picks an entry to paste,
# :yanks clear wipes the history.
enabled = false
# Entries kept, newest first (capped at 100).
max_entries = 20
# Copies larger than this many bytes are not recorded (capped at 65536).
max_entry_bytes = 4096

[dashboard]
# Icons next to the dashboard entries.
#   "auto": common Unicode symbols (no special font needed); disabled on the
#           Linux console, non-UTF-8 locales and legacy Windows consoles.
#   "on":   the same symbols, forced.
#   "nerd": Nerd Font glyphs. Needs a patched font (nerdfonts.com); never
#           chosen automatically because support can't be detected.
#   "off":  no icons.
icons = "auto"
# Show the bottom status bar on the dashboard. false hides it (it still
# appears for prompts, help and status messages).
status_bar = true
# Workstation mode only.
#   "center": actions on top, pins and recent files below (default).
#   "left":   actions panel beside wide file lists.
#   "right":  branding on the left, one stacked panel on the right (needs 100+ columns).
# Layouts fall back to "center" when the terminal is too small.
layout = "center"
# Center layout only: wrap it in a bordered box.
boxed = false

[tabs]
# Language icons on the buffer bar: "auto" = letter badges (RS, PY, ...) that
# work in any font, "nerd" = Nerd Font glyphs, "off" = none.
icons = "auto"
# A second row under the tabs, lit in the language's color below the active tab.
active_line = true

[theme]
# "solid": paint the colors below, whatever the terminal's own background is.
# "transparent": paint nothing, so the terminal's background shows through.
#   For blur: make the terminal translucent (kitty: background_opacity 0.85,
#   alacritty: window.opacity) and enable blur in Hyprland, e.g.
#   decoration { blur { enabled = true } } and a windowrulev2 for the terminal.
#   Bars and highlights stay opaque.
# "auto": transparent under Hyprland, solid everywhere else.
mode = "auto"
background = "#161a22"
foreground = "#c5cfdb"

[completion]
# Input mode popup. Tab accepts, Down/Up/C-n/C-p pick, Esc closes.
enabled = true
# Show the popup while typing. false = only when you press Tab after a word.
auto = true
# Letters typed before the popup appears (Tab always works from 1).
min_chars = 2
max_items = 8
# The popup unrolls when it opens. Needs [ui] animations = true.
animate = true

[notifications]
# Toasts in the top-right corner (off automatically in compact mode).
enabled = true
max_visible = 4
# Base time on screen; warnings last 1.4x and errors 2x as long.
timeout_ms = 3000
# Deleting or pasting at least this many lines in one move raises a notice (0 = off).
big_edit_lines = 10

[keys.all]
# Custom keybinds. One table per mode: [keys.control], [keys.input],
# [keys.mark], or [keys.all] (every mode). The value is a built-in action
# name, a ":" ex command, a '\name' custom command, or a list of those that
# run in order. :keys shows your binds and every action name.
f5 = ":w"                      # save
alt-p = ":path"                # path

[keys.control]
ctrl-j = "move-down"
alt-s = '\save-quit'         # the custom command from [commands]
f6 = ["undo", ":noh"]

[keys.input]
alt-d = "delete-line"

[commands]
write = "w"                    # alias for :w
quit = "q"                     # alias for :q
line-nu = "nu"                 # alias for :nu
force-quit = "q!"              # alias for :q!
save-quit = "wq"               # alias for :wq
file-manager = "e"             # alias for :e
file-manager-force = "e!"      # alias for :e!
settings = "config"            # alias for :config
history = "yanks"              # alias for :yanks
save-quiet = ["w", "noh"]      # runs :w, then :noh
save-reload = ["w", "reload"]  # save and reload
trim = "%s/ +$//g"             # no {} -> typed args are appended
swap = "%s/{}/x/g"             # {} is all typed args
replace = "%s/{1}/{2}/g"       # \replace true false -> :%s/true/false/g
home = "dash"                  # :dashboard / :dash alias
left = "layout left"
center = "layout center"
right = "layout right"
"##;

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
            if ensure_file().is_err() || !path.exists() {
                return (Self::default(), None);
            }
        }
        let text = match read_limited(&path, MAX_CONFIG_BYTES) {
            Ok(text) => text,
            Err(reason) => {
                return (Self::default(), Some(format!("ERR: config.toml: {reason}")));
            }
        };
        match ::toml::from_str::<Self>(&text) {
            Ok(mut config) => match config.check_limits() {
                Ok(()) => {
                    let (keymap, problems) = KeyMap::build(&config.keys, &config.commands);
                    config.keymap = keymap;
                    let note = problems.first().map(|first| {
                        match problems.len().saturating_sub(1) {
                            0 => format!("ERR: config.toml: {first}"),
                            more => format!("ERR: config.toml: {first} (+{more} more)"),
                        }
                    });
                    (config, note)
                }
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
        let tables = [&self.keys.all, &self.keys.control, &self.keys.input, &self.keys.mark];
        if tables.iter().map(|table| table.len()).sum::<usize>() > MAX_KEYBINDS {
            return Err(format!("too many keybinds (limit {MAX_KEYBINDS})"));
        }
        for table in tables {
            for (key, command) in table {
                let steps = command.steps();
                if steps.len() > MAX_COMMAND_LINES
                    || steps.iter().any(|step| step.len() > MAX_COMMAND_LEN)
                {
                    return Err(format!(
                        "keybind `{key}`: at most {MAX_COMMAND_LINES} steps of {MAX_COMMAND_LEN} bytes"
                    ));
                }
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
        assert!(config.scope.guide && !config.scope.highlight && config.scope.brackets);
        assert!(!config.yanks.enabled && config.yanks.max_entries == 20);
        assert!(config.tabs.active_line && config.theme.mode == BackgroundMode::Auto);
        assert!(config.ui.mode == UiMode::Workstation && !config.statusbar.full_path);
        assert!(parse_hex(&config.theme.background).is_some());
        assert!(config.keys.all.contains_key("f5"));
        let (_, problems) = KeyMap::build(&config.keys, &config.commands);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(config.check_limits().is_ok());
    }

    #[test]
    fn only_the_guide_is_on_by_default() {
        let config = Config::default();
        assert!(config.scope.guide && !config.scope.highlight && !config.scope.brackets);
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
    fn hex_colors() {
        assert_eq!(parse_hex("#1a1e27"), Some((0x1a, 0x1e, 0x27)));
        assert_eq!(parse_hex("1a1e27"), Some((0x1a, 0x1e, 0x27)));
        assert_eq!(parse_hex("#1a1e2"), None);
        assert_eq!(parse_hex("+a1e27f"), None);
        assert_eq!(parse_hex(""), None);
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