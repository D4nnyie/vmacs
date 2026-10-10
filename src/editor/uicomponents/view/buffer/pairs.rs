use super::super::highlighter::Language;
use super::{Buffer, TsCache};
use crate::prelude::*;

const MAX_PAIR_SCAN_LINES: usize = 5000;
const MAX_PAIR_ATTEMPTS: usize = 16;

fn bracket_char(grapheme: &str) -> Option<char> {
    let mut chars = grapheme.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

const fn closer_for(c: char) -> Option<char> {
    match c {
        '{' => Some('}'),
        '(' => Some(')'),
        '[' => Some(']'),
        _ => None,
    }
}

const fn opener_for(c: char) -> Option<char> {
    match c {
        '}' => Some('{'),
        ')' => Some('('),
        ']' => Some('['),
        _ => None,
    }
}

const fn is_closer(c: char) -> bool {
    matches!(c, '}' | ')' | ']')
}

fn char_literal_len(rest: &str) -> Option<usize> {
    let mut chars = rest.char_indices();
    chars.next().filter(|(_, c)| *c == '\'')?;
    let (_, first) = chars.next()?;
    if first == '\\' {
        chars.next()?;
        let (idx, _) = chars.by_ref().take(10).find(|(_, c)| *c == '\'')?;
        Some(idx.saturating_add(1))
    } else {
        let (idx, c) = chars.next()?;
        (c == '\'').then_some(idx.saturating_add(1))
    }
}

fn string_len(rest: &str, name: &str, delimiters: &[(String, String)]) -> Option<usize> {
    let (open, close) = delimiters
        .iter()
        .filter(|(open, _)| !open.is_empty() && rest.starts_with(open.as_str()))
        .max_by_key(|(open, _)| open.len())?;
    let escapes = !(open.starts_with('r')
        || open.starts_with('R')
        || open.as_str() == "[["
        || (open.as_str() == "`" && name == "Go"));
    let body = &rest[open.len()..];
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        if escapes && c == '\\' {
            chars.next();
            continue;
        }
        if body[i..].starts_with(close.as_str()) {
            return Some(open.len().saturating_add(i).saturating_add(close.len()));
        }
    }
    Some(rest.len())
}

fn code_brackets(text: &str, lang: Option<&Language>) -> Vec<(ByteIdx, char)> {
    let config = lang.map(|lang| &lang.config);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rest) = text.get(i..).filter(|rest| !rest.is_empty()) {
        if let Some(cfg) = config {
            if let Some((open, close)) = cfg.block_comment.as_ref() {
                if !open.is_empty() && rest.starts_with(open.as_str()) {
                    match rest[open.len()..].find(close.as_str()) {
                        Some(p) => {
                            i = i
                                .saturating_add(open.len())
                                .saturating_add(p)
                                .saturating_add(close.len());
                            continue;
                        }
                        None => break,
                    }
                }
            }
            if cfg
                .line_comment
                .as_deref()
                .is_some_and(|prefix| !prefix.is_empty() && rest.starts_with(prefix))
            {
                break;
            }
            if let Some(n) = string_len(rest, &cfg.name, &cfg.string_delimiters) {
                i = i.saturating_add(n);
                continue;
            }
            if cfg.supports_char_literal {
                if let Some(n) = char_literal_len(rest) {
                    i = i.saturating_add(n);
                    continue;
                }
            }
        }
        let Some(c) = rest.chars().next() else {
            break;
        };
        if matches!(c, '{' | '}' | '(' | ')' | '[' | ']') {
            out.push((i, c));
        }
        i = i.saturating_add(c.len_utf8());
    }
    out
}

impl Buffer {
    pub fn char_before(&self, at: Location) -> Option<char> {
        let idx = at.grapheme_idx.checked_sub(1)?;
        self.lines.get(at.line_idx)?.grapheme_at(idx)?.chars().last()
    }

    pub fn char_after(&self, at: Location) -> Option<char> {
        self.lines
            .get(at.line_idx)?
            .grapheme_at(at.grapheme_idx)?
            .chars()
            .next()
    }

    pub fn supports_lifetime(&self) -> bool {
        self.file_info
            .get_language()
            .is_some_and(|lang| lang.config.supports_lifetime)
    }

    pub fn is_in_string(&self, at: Location) -> bool {
        self.in_string(at).unwrap_or(false)
    }

    fn in_string(&self, at: Location) -> Option<bool> {
        self.file_info.get_language()?.ts.as_ref()?;
        let cache = self
            .ts_cache
            .as_ref()
            .filter(|cache| cache.is_current(self.revision))?;
        let line = self.lines.get(at.line_idx)?;
        let line_start = cache.line_start(at.line_idx)?;
        let byte = line
            .byte_range_for_grapheme_range(at.grapheme_idx..at.grapheme_idx)
            .start;
        Some(cache.is_inside_string(line_start.saturating_add(byte)))
    }

    pub fn rest_is_unbalanced(&self, at: Location, opener: char, closer: char) -> bool {
        let Some(line) = self.lines.get(at.line_idx) else {
            return false;
        };
        let rest = line.substring(at.grapheme_idx..line.grapheme_count());
        let closers = rest.chars().filter(|c| *c == closer).count();
        let openers = rest.chars().filter(|c| *c == opener).count();
        closers > openers
    }

    pub fn is_between_autopair(&self, at: Location) -> bool {
        matches!(
            (self.char_before(at), self.char_after(at)),
            (Some('('), Some(')'))
                | (Some('['), Some(']'))
                | (Some('{'), Some('}'))
                | (Some('"'), Some('"'))
                | (Some('\''), Some('\''))
                | (Some('`'), Some('`'))
        )
    }

    fn tree_pair(&self, at: Location) -> Option<Option<(Location, Location)>> {
        self.file_info.get_language()?.ts.as_ref()?;
        let cache = self
            .ts_cache
            .as_ref()
            .filter(|cache| cache.is_current(self.revision))?;
        if cache.has_error() {
            return None;
        }
        let line = self.lines.get(at.line_idx)?;
        let line_start = cache.line_start(at.line_idx)?;
        let byte = line
            .byte_range_for_grapheme_range(at.grapheme_idx..at.grapheme_idx)
            .start;
        let found = cache.bracket_pair(line_start.saturating_add(byte));
        Some(found.and_then(|(open, close)| {
            Some((
                self.location_for_abs_byte(cache, open)?,
                self.location_for_abs_byte(cache, close)?,
            ))
        }))
    }

    pub fn enclosing_pair(&self, at: Location) -> Option<(Location, Location)> {
        match self.tree_pair(at) {
            Some(found) => found,
            None => self.scan_pair(at),
        }
    }

    fn location_for_abs_byte(&self, cache: &TsCache, abs: usize) -> Option<Location> {
        let line_idx = cache.line_of_byte(abs)?;
        let start = cache.line_start(line_idx)?;
        let line = self.lines.get(line_idx)?;
        let grapheme_idx = line.byte_idx_to_grapheme_idx(abs.saturating_sub(start))?;
        Some(Location { line_idx, grapheme_idx })
    }

    fn tree_cache(&self) -> Option<&TsCache> {
        self.file_info.get_language()?.ts.as_ref()?;
        self.ts_cache
            .as_ref()
            .filter(|cache| cache.is_current(self.revision))
    }

    fn line_brackets(&self, line_idx: LineIdx) -> Vec<(GraphemeIdx, char)> {
        let Some(line) = self.lines.get(line_idx) else {
            return Vec::new();
        };
        let tree = self
            .tree_cache()
            .and_then(|cache| Some((cache, cache.line_start(line_idx)?)));
        let found: Vec<(ByteIdx, char)> = match tree {
            Some((cache, start)) => line
                .char_indices()
                .filter(|(_, c)| matches!(c, '{' | '}' | '(' | ')' | '[' | ']'))
                .filter(|(byte, _)| cache.is_code_byte(start.saturating_add(*byte)))
                .collect(),
            None => code_brackets(line, self.file_info.get_language().map(|lang| &**lang)),
        };
        found
            .into_iter()
            .filter_map(|(byte, c)| {
                let g = line.byte_idx_to_grapheme_idx(byte)?;
                (line.grapheme_at(g).and_then(bracket_char) == Some(c)).then_some((g, c))
            })
            .collect()
    }

    fn scan_pair(&self, at: Location) -> Option<(Location, Location)> {
        let here = self
            .line_brackets(at.line_idx)
            .into_iter()
            .find(|(g, _)| *g == at.grapheme_idx)
            .map(|(_, c)| c);
        match here {
            Some(c) if closer_for(c).is_some() => {
                let closer = self.find_closer(at, c)?;
                return Some((at, closer));
            }
            Some(c) => {
                let (opener, _) = self.find_opener(at, Some(c))?;
                return Some((opener, at));
            }
            None => {}
        }
        let mut from = at;
        for _ in 0..MAX_PAIR_ATTEMPTS {
            let (opener, open_char) = self.find_opener(from, None)?;
            if let Some(closer) = self.find_closer(opener, open_char) {
                if (closer.line_idx, closer.grapheme_idx) > (at.line_idx, at.grapheme_idx) {
                    return Some((opener, closer));
                }
            }
            from = opener;
        }
        None
    }

    fn find_opener(&self, from: Location, closer: Option<char>) -> Option<(Location, char)> {
        let floor = from.line_idx.saturating_sub(MAX_PAIR_SCAN_LINES);
        let mut stack: Vec<char> = closer.and_then(opener_for).into_iter().collect();
        let seeded = !stack.is_empty();
        let mut line_idx = from.line_idx;
        loop {
            for (g, c) in self.line_brackets(line_idx).into_iter().rev() {
                if line_idx == from.line_idx && g >= from.grapheme_idx {
                    continue;
                }
                if let Some(wanted) = opener_for(c) {
                    stack.push(wanted);
                } else if closer_for(c).is_some() {
                    match stack.iter().rposition(|&expected| expected == c) {
                        Some(0) if seeded => {
                            return Some((Location { line_idx, grapheme_idx: g }, c));
                        }
                        Some(pos) => stack.truncate(pos),
                        None if !seeded => {
                            return Some((Location { line_idx, grapheme_idx: g }, c));
                        }
                        None => {}
                    }
                }
            }
            if line_idx <= floor {
                return None;
            }
            line_idx = line_idx.saturating_sub(1);
        }
    }

    fn find_closer(&self, opener: Location, open: char) -> Option<Location> {
        let mut stack = vec![closer_for(open)?];
        let last = opener
            .line_idx
            .saturating_add(MAX_PAIR_SCAN_LINES)
            .min(self.lines.len().saturating_sub(1));
        for line_idx in opener.line_idx..=last {
            for (g, c) in self.line_brackets(line_idx) {
                if line_idx == opener.line_idx && g <= opener.grapheme_idx {
                    continue;
                }
                if let Some(expected) = closer_for(c) {
                    stack.push(expected);
                } else if is_closer(c) {
                    if let Some(pos) = stack.iter().rposition(|&e| e == c) {
                        if pos == 0 {
                            return Some(Location { line_idx, grapheme_idx: g });
                        }
                        stack.truncate(pos);
                    }
                }
            }
        }
        None
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn loc(line_idx: usize, grapheme_idx: usize) -> Location {
        Location { line_idx, grapheme_idx }
    }

    fn open(dir: &TempDir, name: &str, text: &str) -> Buffer {
        let path = dir.write(name, text.as_bytes());
        Buffer::load(path.to_str().unwrap()).unwrap()
    }

    fn pair(buffer: &Buffer, at: Location) -> Option<((usize, usize), (usize, usize))> {
        buffer
            .enclosing_pair(at)
            .map(|(a, b)| ((a.line_idx, a.grapheme_idx), (b.line_idx, b.grapheme_idx)))
    }

    #[test]
    fn innermost_enclosing_pair() {
        let dir = TempDir::new();
        let buffer = open(&dir, "f.txt", "(a [b] c)\n");
        assert_eq!(pair(&buffer, loc(0, 4)), Some(((0, 3), (0, 5))));
        assert_eq!(pair(&buffer, loc(0, 7)), Some(((0, 0), (0, 8))));
    }

    #[test]
    fn cursor_on_a_bracket_selects_its_pair() {
        let dir = TempDir::new();
        let buffer = open(&dir, "f.txt", "(a [b] c)\n");
        assert_eq!(pair(&buffer, loc(0, 3)), Some(((0, 3), (0, 5))));
        assert_eq!(pair(&buffer, loc(0, 8)), Some(((0, 0), (0, 8))));
    }

    #[test]
    fn crossing_brackets_pair_by_type() {
        let dir = TempDir::new();
        let buffer = open(&dir, "f.txt", "([)]\n");
        assert_eq!(pair(&buffer, loc(0, 0)), Some(((0, 0), (0, 2))));
        assert_eq!(pair(&buffer, loc(0, 1)), Some(((0, 1), (0, 3))));
        assert_eq!(pair(&buffer, loc(0, 2)), Some(((0, 0), (0, 2))));
    }

    #[test]
    fn no_enclosing_pair() {
        let dir = TempDir::new();
        assert_eq!(pair(&open(&dir, "a.txt", "abc\n"), loc(0, 1)), None);
        assert_eq!(pair(&open(&dir, "b.txt", "(a) b\n"), loc(0, 4)), None);
    }

    #[test]
    fn multi_line_pair() {
        let dir = TempDir::new();
        let buffer = open(&dir, "f.txt", "{\n  x\n}\n");
        assert_eq!(pair(&buffer, loc(1, 2)), Some(((0, 0), (2, 0))));
    }

    #[test]
    fn brackets_in_strings_and_comments_are_ignored() {
        let dir = TempDir::new();
        // C# has no parser, so this exercises the heuristic scanner.
        let buffer = open(&dir, "a.cs", "foo(\"(\", x)\n");
        assert_eq!(pair(&buffer, loc(0, 9)), Some(((0, 3), (0, 10))));

        let buffer = open(&dir, "b.cs", "foo(x // )\n);\n");
        assert_eq!(pair(&buffer, loc(0, 4)), Some(((0, 3), (1, 0))));
    }

    #[test]
    fn tree_path_skips_strings() {
        let dir = TempDir::new();
        let text = "fn f() { let s = \"(}\"; g(1); }\n";
        let mut buffer = open(&dir, "f.rs", text);
        buffer.refresh_syntax();

        let one = text.find('1').unwrap();
        let open_paren = text.find("g(").unwrap() + 1;
        assert_eq!(pair(&buffer, loc(0, one)), Some(((0, open_paren), (0, one + 1))));

        // Cursor inside the string: the enclosing block, not the string's own brackets.
        let in_string = text.find("\"(}\"").unwrap() + 1;
        let open_brace = text.find('{').unwrap();
        let close_brace = text.rfind('}').unwrap();
        assert_eq!(pair(&buffer, loc(0, in_string)), Some(((0, open_brace), (0, close_brace))));
    }

    #[test]
    fn multi_line_string_brackets_are_ignored_even_with_syntax_errors() {
        let dir = TempDir::new();
        let text = "fn f() {\nlet s = \"\n(\n\";\ng(x);\n}\n)))\n";
        let mut buffer = open(&dir, "f.rs", text);
        buffer.refresh_syntax();
        assert_eq!(pair(&buffer, loc(4, 0)), Some(((0, 7), (5, 0))));
    }

    #[test]
    fn autopair_helpers() {
        let dir = TempDir::new();
        let buffer = open(&dir, "f.txt", "()\n\"\"\nab\n");
        assert!(buffer.is_between_autopair(loc(0, 1)));
        assert!(buffer.is_between_autopair(loc(1, 1)));
        assert!(!buffer.is_between_autopair(loc(2, 1)));
        assert_eq!(buffer.char_before(loc(0, 1)), Some('('));
        assert_eq!(buffer.char_after(loc(0, 1)), Some(')'));
        assert!(buffer.rest_is_unbalanced(loc(2, 0), '(', ')') == false);
    }
}
