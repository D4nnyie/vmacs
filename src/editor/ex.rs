#![allow(clippy::arithmetic_side_effects)]

#[derive(Clone, Copy, Debug)]
pub enum Base {
    Line(usize),
    Current,
    Last,
    MarkStart,
    MarkEnd,
}

#[derive(Clone, Copy, Debug)]
pub struct Addr {
    base: Base,
    offset: isize,
}

#[derive(Clone, Copy, Debug)]
pub enum LineRange {
    Current,
    All,
    Span(Addr, Addr),
}

pub struct RangeCtx {
    pub current: usize,
    pub total: usize,
    pub marks: Option<(usize, usize)>,
}

pub enum RepPart {
    Text(String),
    Matched,
}

use unicode_segmentation::GraphemeCursor;

fn on_grapheme_boundary(text: &str, idx: usize) -> bool {
    GraphemeCursor::new(idx, text.len(), true)
        .is_boundary(text, 0)
        .unwrap_or(true)
}

pub struct Substitute {
    pub pattern: String,
    pub replacement: Vec<RepPart>,
    pub global: bool,
    pub ignore_case: bool,
    pub count_only: bool,
}

impl Addr {
    const CURRENT: Self = Self { base: Base::Current, offset: 0 };

    fn resolve(self, ctx: &RangeCtx) -> Result<usize, String> {
        let base = match self.base {
            Base::Line(n) => n,
            Base::Current => ctx.current + 1,
            Base::Last => ctx.total,
            Base::MarkStart => ctx.marks.ok_or("E20: Mark not set")?.0 + 1,
            Base::MarkEnd => ctx.marks.ok_or("E20: Mark not set")?.1 + 1,
        };
        let line = isize::try_from(base).unwrap_or(isize::MAX).saturating_add(self.offset);
        match usize::try_from(line) {
            Ok(n) if n >= 1 && n <= ctx.total => Ok(n - 1),
            _ => Err(format!("E16: Invalid range: line {line} (buffer has {} lines)", ctx.total)),
        }
    }
}

impl LineRange {
    pub fn resolve(self, ctx: &RangeCtx) -> Result<(usize, usize), String> {
        if ctx.total == 0 {
            return Err("E16: Invalid range: the buffer is empty".to_string());
        }
        let (a, b) = match self {
            Self::Current => {
                let line = ctx.current.min(ctx.total - 1);
                (line, line)
            }
            Self::All => (0, ctx.total - 1),
            Self::Span(a, b) => (a.resolve(ctx)?, b.resolve(ctx)?),
        };
        Ok(if a <= b { (a, b) } else { (b, a) })
    }
}

fn parse_addr(input: &str) -> Result<(Option<Addr>, &str), String> {
    let mut rest = input;
    let base = if let Some(r) = rest.strip_prefix('.') {
        rest = r;
        Some(Base::Current)
    } else if let Some(r) = rest.strip_prefix('$') {
        rest = r;
        Some(Base::Last)
    } else if let Some(r) = rest.strip_prefix("'<") {
        rest = r;
        Some(Base::MarkStart)
    } else if let Some(r) = rest.strip_prefix("'>") {
        rest = r;
        Some(Base::MarkEnd)
    } else {
        let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0 {
            let n = rest[..digits]
                .parse::<usize>()
                .map_err(|_| "E16: Invalid range".to_string())?;
            rest = &rest[digits..];
            Some(Base::Line(n))
        } else {
            None
        }
    };

    let mut offset: isize = 0;
    let mut has_offset = false;
    loop {
        let sign: isize = match rest.as_bytes().first() {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => break,
        };
        rest = &rest[1..];
        let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        let n = if digits == 0 {
            1
        } else {
            rest[..digits]
                .parse::<isize>()
                .map_err(|_| "E16: Invalid range".to_string())?
        };
        rest = &rest[digits..];
        offset = offset.saturating_add(sign.saturating_mul(n));
        has_offset = true;
    }

    let addr = match base {
        Some(base) => Some(Addr { base, offset }),
        None if has_offset => Some(Addr { base: Base::Current, offset }),
        None => None,
    };
    Ok((addr, rest))
}

pub fn parse_range(input: &str) -> Result<(LineRange, &str), String> {
    if let Some(rest) = input.strip_prefix('%') {
        return Ok((LineRange::All, rest));
    }
    let (first, rest) = parse_addr(input)?;
    if let Some(after) = rest.strip_prefix(',') {
        let (second, rest) = parse_addr(after)?;
        return Ok((
            LineRange::Span(first.unwrap_or(Addr::CURRENT), second.unwrap_or(Addr::CURRENT)),
            rest,
        ));
    }
    Ok(match first {
        Some(addr) => (LineRange::Span(addr, addr), rest),
        None => (LineRange::Current, rest),
    })
}

fn take_part(input: &str, delim: char) -> (String, Option<&str>) {
    let mut out = String::new();
    let mut chars = input.char_indices();
    while let Some((index, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, next)) if next == delim => out.push(next),
                Some((_, next)) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else if c == delim {
            return (out, Some(&input[index + c.len_utf8()..]));
        } else {
            out.push(c);
        }
    }
    (out, None)
}

fn unescape_pattern(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some(next) if next.is_ascii_punctuation() => out.push(next),
            Some(next) => {
                out.push('\\');
                out.push(next);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_replacement(raw: &str) -> Vec<RepPart> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '&' => {
                if !text.is_empty() {
                    parts.push(RepPart::Text(std::mem::take(&mut text)));
                }
                parts.push(RepPart::Matched);
            }
            '\\' => match chars.next() {
                Some('&') => text.push('&'),
                Some('\\') => text.push('\\'),
                Some('t') => text.push('\t'),
                Some(other) => {
                    text.push('\\');
                    text.push(other);
                }
                None => text.push('\\'),
            },
            _ => text.push(c),
        }
    }
    if !text.is_empty() {
        parts.push(RepPart::Text(text));
    }
    parts
}

pub fn parse_substitute(input: &str) -> Result<Option<Substitute>, String> {
    let Some(rest) = input.strip_prefix('s') else {
        return Ok(None);
    };
    let Some(delim) = rest.chars().next() else {
        return Err("Usage: :s/pattern/replacement/[gi]".to_string());
    };
    if delim.is_alphanumeric() || delim.is_whitespace() || matches!(delim, '\\' | '"' | '|') {
        return Ok(None);
    }
    let body = &rest[delim.len_utf8()..];
    let (raw_pattern, after) = take_part(body, delim);
    let (raw_replacement, flags) = match after {
        Some(after) => {
            let (replacement, flags) = take_part(after, delim);
            (replacement, flags.unwrap_or(""))
        }
        None => (String::new(), ""),
    };
    let mut sub = Substitute {
        pattern: unescape_pattern(&raw_pattern),
        replacement: parse_replacement(&raw_replacement),
        global: false,
        ignore_case: false,
        count_only: false,
    };
    for flag in flags.trim().chars() {
        match flag {
            'g' => sub.global = true,
            'i' => sub.ignore_case = true,
            'I' => sub.ignore_case = false,
            'n' => sub.count_only = true,
            'c' => return Err("The c (confirm) flag is not supported".to_string()),
            _ => return Err(format!("E488: Trailing characters: {flags}")),
        }
    }
    Ok(Some(sub))
}

pub fn replace_in_line(line: &str, sub: &Substitute) -> Option<(String, usize)> {
    if sub.pattern.is_empty() {
        return None;
    }
    let (haystack, needle) = if sub.ignore_case {
        (line.to_ascii_lowercase(), sub.pattern.to_ascii_lowercase())
    } else {
        (line.to_string(), sub.pattern.clone())
    };
    let mut out = String::with_capacity(line.len());
    let (mut pos, mut count) = (0usize, 0usize);
    while let Some(found) = haystack[pos..].find(&needle) {
        let start = pos + found;
        let end = start + needle.len();
        if !(on_grapheme_boundary(line, start) && on_grapheme_boundary(line, end)) {
            let step = line[start..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&line[pos..start + step]);
            pos = start + step;
            continue;
        }
        out.push_str(&line[pos..start]);
        for part in &sub.replacement {
            match part {
                RepPart::Text(text) => out.push_str(text),
                RepPart::Matched => out.push_str(&line[start..end]),
            }
        }
        pos = end;
        count += 1;
        if !sub.global {
            break;
        }
    }
    if count == 0 {
        return None;
    }
    out.push_str(&line[pos..]);
    Some((out, count))
}


#[cfg(test)]
mod tests {
    use super::*;

    const CTX: RangeCtx = RangeCtx { current: 4, total: 10, marks: Some((2, 5)) };

    fn resolve(input: &str) -> Result<(usize, usize), String> {
        let (range, rest) = parse_range(input)?;
        assert!(rest.is_empty(), "leftover: {rest:?}");
        range.resolve(&CTX)
    }

    fn sub(input: &str) -> Substitute {
        parse_substitute(input).unwrap().unwrap()
    }

    #[test]
    fn ranges_resolve() {
        assert_eq!(resolve("%"), Ok((0, 9)));
        assert_eq!(resolve("3"), Ok((2, 2)));
        assert_eq!(resolve("2,5"), Ok((1, 4)));
        assert_eq!(resolve("."), Ok((4, 4)));
        assert_eq!(resolve("$"), Ok((9, 9)));
        assert_eq!(resolve(".,$"), Ok((4, 9)));
        assert_eq!(resolve("'<,'>"), Ok((2, 5)));
        assert_eq!(resolve(".+2"), Ok((6, 6)));
    }

    #[test]
    fn reversed_range_is_swapped() {
        assert_eq!(resolve("5,2"), Ok((1, 4)));
    }

    #[test]
    fn out_of_range_is_an_error() {
        assert!(resolve("11").is_err());
        assert!(resolve("0").is_err());
        let empty = RangeCtx { current: 0, total: 0, marks: None };
        assert!(LineRange::All.resolve(&empty).is_err());
    }

    #[test]
    fn marks_require_a_selection() {
        let ctx = RangeCtx { current: 0, total: 10, marks: None };
        let (range, _) = parse_range("'<,'>").unwrap();
        assert!(range.resolve(&ctx).is_err());
    }

    #[test]
    fn parses_substitute() {
        let s = sub("s/a/b/g");
        assert_eq!(s.pattern, "a");
        assert!(s.global && !s.ignore_case && !s.count_only);
        assert!(matches!(s.replacement.as_slice(), [RepPart::Text(t)] if t == "b"));
    }

    #[test]
    fn substitute_delimiters_and_escapes() {
        assert_eq!(sub("s#a/b#x#").pattern, "a/b");
        assert_eq!(sub("s/a\\/b/x/").pattern, "a/b");
    }

    #[test]
    fn substitute_flags() {
        let s = sub("s/a/b/gin");
        assert!(s.global && s.ignore_case && s.count_only);
    }

    #[test]
    fn not_a_substitute() {
        assert!(parse_substitute("sort").unwrap().is_none());
        assert!(parse_substitute("s").is_err());
        assert!(parse_substitute("s/a/b/c").is_err());
        assert!(parse_substitute("s/a/b/q").is_err());
    }

    #[test]
    fn replaces_first_or_all() {
        assert_eq!(replace_in_line("aaa", &sub("s/a/x/")), Some(("xaa".to_string(), 1)));
        assert_eq!(replace_in_line("aaa", &sub("s/a/x/g")), Some(("xxx".to_string(), 3)));
        assert_eq!(replace_in_line("bbb", &sub("s/a/x/g")), None);
    }

    #[test]
    fn ampersand_is_the_match() {
        assert_eq!(replace_in_line("abc", &sub("s/b/[&]/")), Some(("a[b]c".to_string(), 1)));
        assert_eq!(replace_in_line("abc", &sub("s/b/\\&/")), Some(("a&c".to_string(), 1)));
    }

    #[test]
    fn ignore_case() {
        assert_eq!(replace_in_line("aA", &sub("s/A/x/i")), Some(("xA".to_string(), 1)));
    }

    #[test]
    fn never_splits_a_grapheme_cluster() {
        assert_eq!(replace_in_line("e\u{301}", &sub("s/e/x/")), None);
    }

    #[test]
    fn empty_pattern_matches_nothing() {
        assert_eq!(replace_in_line("abc", &sub("s//x/")), None);
    }
}