#![allow(clippy::arithmetic_side_effects)]

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::command::find_action;
use super::config::{CustomCommand, KeysConfig};
use super::mode::Mode;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Chord {
    code: KeyCode,
    mods: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    All,
    Control,
    Input,
    Mark,
}

impl Section {
    const ORDER: [Self; 4] = [Self::All, Self::Control, Self::Input, Self::Mark];

    pub const fn title(self) -> &'static str {
        match self {
            Self::All => "ALL MODES",
            Self::Control => "CONTROL MODE",
            Self::Input => "INPUT MODE",
            Self::Mark => "MARK MODE",
        }
    }

    const fn table(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Control => "control",
            Self::Input => "input",
            Self::Mark => "mark",
        }
    }

    fn source(self, keys: &KeysConfig) -> &std::collections::HashMap<String, CustomCommand> {
        match self {
            Self::All => &keys.all,
            Self::Control => &keys.control,
            Self::Input => &keys.input,
            Self::Mark => &keys.mark,
        }
    }
}

#[derive(Clone)]
pub struct BindInfo {
    pub section: Section,
    pub key: String,
    pub action: String,
}

#[derive(Clone, Default)]
pub struct KeyMap {
    all: HashMap<Chord, Vec<String>>,
    control: HashMap<Chord, Vec<String>>,
    input: HashMap<Chord, Vec<String>>,
    mark: HashMap<Chord, Vec<String>>,
    listing: Vec<BindInfo>,
}

fn normalize(code: KeyCode, modifiers: KeyModifiers) -> Chord {
    let mut mods = modifiers;
    if mods.contains(KeyModifiers::META) {
        mods.remove(KeyModifiers::META);
        mods.insert(KeyModifiers::ALT);
    }
    let shift = mods.contains(KeyModifiers::SHIFT);
    let code = match code {
        KeyCode::Char(c) => {
            mods.remove(KeyModifiers::SHIFT);
            let c = if !c.is_alphabetic() {
                c
            } else if shift {
                c.to_uppercase().next().unwrap_or(c)
            } else if mods.contains(KeyModifiers::CONTROL) {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c
            };
            KeyCode::Char(c)
        }
        KeyCode::BackTab => {
            mods.insert(KeyModifiers::SHIFT);
            KeyCode::Tab
        }
        other => other,
    };
    Chord { code, mods: mods.bits() }
}

fn key_code(name: &str) -> Option<KeyCode> {
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => KeyCode::Char(' '),
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        _ => {
            if let Some(n) = lower
                .strip_prefix('f')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|n| (1..=24).contains(n))
            {
                return Some(KeyCode::F(n));
            }
            let mut chars = name.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Some(KeyCode::Char(c)),
                _ => None,
            };
        }
    };
    Some(named)
}

fn parse_chord(spec: &str) -> Result<Chord, String> {
    let spec = spec.trim();
    let mut mods = KeyModifiers::NONE;
    let mut rest = spec;
    while let Some(dash) = rest.find('-') {
        let (prefix, tail) = (&rest[..dash], &rest[dash + 1..]);
        if prefix.is_empty() || tail.is_empty() {
            break;
        }
        mods |= match prefix.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "c" => KeyModifiers::CONTROL,
            "alt" | "meta" | "m" => KeyModifiers::ALT,
            "shift" | "s" => KeyModifiers::SHIFT,
            _ => return Err(format!("unknown modifier `{prefix}`")),
        };
        rest = tail;
    }
    let code = key_code(rest).ok_or_else(|| format!("unknown key `{rest}`"))?;
    Ok(normalize(code, mods))
}

fn check_reserved(section: Section, key: Chord) -> Result<(), String> {
    let mods = KeyModifiers::from_bits_truncate(key.mods);
    if key.code == KeyCode::Esc
        || key.code == KeyCode::F(1)
        || key == normalize(KeyCode::Char('h'), KeyModifiers::ALT)
    {
        return Err("reserved (Esc, F1 and alt-h cannot be rebound)".to_string());
    }
    let types_text = matches!(section, Section::Input | Section::All);
    if types_text
        && matches!(key.code, KeyCode::Char(_))
        && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Err("a bare character would stop you typing it; use ctrl- or alt-".to_string());
    }
    Ok(())
}

fn check_step(step: &str, commands: &HashMap<String, CustomCommand>) -> Result<(), String> {
    if let Some(line) = step.strip_prefix(':') {
        if line.trim().is_empty() {
            return Err("empty `:` command".to_string());
        }
    } else if let Some(line) = step.strip_prefix('\\') {
        let name = line.split_whitespace().next().unwrap_or("");
        if !commands.contains_key(name) {
            return Err(format!("unknown custom command `\\{name}` (define it under [commands])"));
        }
    } else if find_action(step).is_none() {
        return Err(format!("unknown action `{step}` (:keys lists them)"));
    }
    Ok(())
}

impl KeyMap {
    pub fn build(
        keys: &KeysConfig,
        commands: &HashMap<String, CustomCommand>,
    ) -> (Self, Vec<String>) {
        let mut map = Self::default();
        let mut problems = Vec::new();
        for section in Section::ORDER {
            let mut entries: Vec<(&String, &CustomCommand)> = section.source(keys).iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (spec, command) in entries {
                if let Err(message) = map.add(section, spec, command, commands) {
                    problems.push(format!("keys.{}: `{spec}`: {message}", section.table()));
                }
            }
        }
        (map, problems)
    }

    fn table_mut(&mut self, section: Section) -> &mut HashMap<Chord, Vec<String>> {
        match section {
            Section::All => &mut self.all,
            Section::Control => &mut self.control,
            Section::Input => &mut self.input,
            Section::Mark => &mut self.mark,
        }
    }

    fn add(
        &mut self,
        section: Section,
        spec: &str,
        command: &CustomCommand,
        commands: &HashMap<String, CustomCommand>,
    ) -> Result<(), String> {
        let key = parse_chord(spec)?;
        check_reserved(section, key)?;
        let steps: Vec<String> = command
            .steps()
            .into_iter()
            .map(|step| step.trim().to_string())
            .collect();
        if steps.is_empty() {
            return Err("needs at least one action".to_string());
        }
        for step in &steps {
            check_step(step, commands)?;
        }
        let table = self.table_mut(section);
        if table.contains_key(&key) {
            return Err("same key as another entry in this table".to_string());
        }
        table.insert(key, steps.clone());
        self.listing.push(BindInfo {
            section,
            key: spec.trim().to_string(),
            action: steps.join(" ; "),
        });
        Ok(())
    }

    pub fn lookup(&self, mode: Mode, event: &KeyEvent) -> Option<&[String]> {
        let key = normalize(event.code, event.modifiers);
        let own = match mode {
            Mode::Normal => &self.control,
            Mode::Insert => &self.input,
            Mode::Visual => &self.mark,
        };
        own.get(&key).or_else(|| self.all.get(&key)).map(Vec::as_slice)
    }

    pub fn listing(&self) -> &[BindInfo] {
        &self.listing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::config::Config;

    fn build(text: &str) -> (KeyMap, Vec<String>) {
        let config: Config = ::toml::from_str(text).expect("valid toml");
        KeyMap::build(&config.keys, &config.commands)
    }

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn chord_syntax() {
        assert_eq!(parse_chord("ctrl-k"), parse_chord("C-K"));
        assert_eq!(parse_chord("alt-x"), parse_chord("M-x"));
        assert_eq!(parse_chord("shift-g"), parse_chord("G"));
        assert_ne!(parse_chord("g"), parse_chord("G"));
        assert!(parse_chord("ctrl--").is_ok());
        assert!(parse_chord("f24").is_ok());
        assert!(parse_chord("f25").is_err());
        assert!(parse_chord("ctrl-").is_err());
        assert!(parse_chord("hyper-x").is_err());
    }

    #[test]
    fn events_match_specs() {
        assert_eq!(
            normalize(KeyCode::Char('k'), KeyModifiers::CONTROL),
            parse_chord("ctrl-k").unwrap()
        );
        assert_eq!(
            normalize(KeyCode::Char('G'), KeyModifiers::SHIFT),
            parse_chord("G").unwrap()
        );
        assert_eq!(
            normalize(KeyCode::Char('x'), KeyModifiers::META),
            parse_chord("alt-x").unwrap()
        );
        assert_eq!(
            normalize(KeyCode::BackTab, KeyModifiers::SHIFT),
            parse_chord("shift-tab").unwrap()
        );
    }

    #[test]
    fn lookup_prefers_the_mode_table_over_all() {
        let (map, problems) = build(
            "[keys.all]\nf5 = \":w\"\nf6 = \"undo\"\n[keys.control]\nf6 = \"redo\"\n",
        );
        assert!(problems.is_empty(), "{problems:?}");
        let f5 = ev(KeyCode::F(5), KeyModifiers::NONE);
        let f6 = ev(KeyCode::F(6), KeyModifiers::NONE);
        assert_eq!(map.lookup(Mode::Insert, &f5), Some(&[":w".to_string()][..]));
        assert_eq!(map.lookup(Mode::Normal, &f6), Some(&["redo".to_string()][..]));
        assert_eq!(map.lookup(Mode::Insert, &f6), Some(&["undo".to_string()][..]));
        assert!(map.lookup(Mode::Normal, &ev(KeyCode::F(7), KeyModifiers::NONE)).is_none());
    }

    #[test]
    fn bad_bindings_are_reported_and_skipped() {
        let (map, problems) = build(
            "[keys.input]\na = \"undo\"\nctrl-j = \"nope\"\nctrl-k = \"undo\"\n\
             [keys.control]\nf6 = '\\missing'\nesc = \"undo\"\n",
        );
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert_eq!(map.listing().len(), 1);
        assert_eq!(map.listing()[0].key, "ctrl-k");
    }

    #[test]
    fn custom_commands_can_be_bound() {
        let (map, problems) =
            build("[commands]\nsave = \"w\"\n[keys.control]\nf6 = '\\save'\n");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(map.listing()[0].action, "\\save");
    }
}