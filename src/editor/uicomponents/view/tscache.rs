#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use super::super::super::{Annotation, AnnotationType, Line};
use super::highlighter::TsConfig;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

pub struct TsCache {
    parser: Parser,
    query: Query,
    tree: Option<Tree>,
    revision: u64,
    line_starts: Vec<usize>,
    per_line: Vec<Vec<Annotation>>,
}

impl TsCache {
    pub fn new(config: &TsConfig) -> Option<Self> {
        let language = (config.language)();
        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        let query = Query::new(&language, &config.highlights.join("\n")).ok()?;
        Some(Self {
            parser,
            query,
            tree: None,
            revision: u64::MAX,
            line_starts: Vec::new(),
            per_line: Vec::new(),
        })
    }

    pub fn is_current(&self, revision: u64) -> bool {
        self.revision == revision && self.tree.is_some()
    }

    pub fn annotations(&self) -> &[Vec<Annotation>] {
        &self.per_line
    }

    pub fn line_start(&self, line_idx: usize) -> Option<usize> {
        self.line_starts.get(line_idx).copied()
    }

    pub fn update(&mut self, lines: &[Line], revision: u64) {
        if self.is_current(revision) {
            return;
        }
        let mut source = String::new();
        let mut line_starts = Vec::with_capacity(lines.len().saturating_add(1));
        for line in lines {
            line_starts.push(source.len());
            source.push_str(line);
            source.push('\n');
        }
        line_starts.push(source.len());

        self.tree = self.parser.parse(&source, None);
        self.line_starts = line_starts;
        self.revision = revision;
        self.per_line = self.collect(&source, lines.len());
    }

    fn collect(&self, source: &str, line_count: usize) -> Vec<Vec<Annotation>> {
        let mut out: Vec<Vec<Annotation>> = vec![Vec::new(); line_count];
        let Some(tree) = &self.tree else {
            return out;
        };
        let names = self.query.capture_names();

        let mut raw: Vec<(usize, usize, AnnotationType)> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(&self.query, tree.root_node(), source.as_bytes());
        while let Some((query_match, capture_idx)) = captures.next() {
            let capture = &query_match.captures()[*capture_idx];
            if let Some(kind) = capture_to_type(names[capture.index as usize]) {
                let range = capture.node.byte_range();
                raw.push((range.start, range.end, kind));
            }
        }

        raw.sort_by_key(|&(start, end, kind)| {
            (kind == AnnotationType::Attribute, start, std::cmp::Reverse(end))
        });

        for (start, end, kind) in raw {
            let first = self
                .line_starts
                .partition_point(|&line_start| line_start <= start)
                .saturating_sub(1);
            for line_idx in first..line_count {
                let line_start = self.line_starts[line_idx];
                if line_start >= end {
                    break;
                }
                let line_end = self.line_starts[line_idx + 1] - 1;
                let a = start.max(line_start) - line_start;
                let b = end.min(line_end) - line_start;
                if a < b {
                    out[line_idx].push(Annotation {
                        annotation_type: kind,
                        start: a,
                        end: b,
                    });
                }
            }
        }
        out
    }

    pub fn is_inside_string(&self, abs_byte: usize) -> bool {
        let Some(tree) = self.tree.as_ref() else {
            return false;
        };
        let mut node = tree.root_node().descendant_for_byte_range(abs_byte, abs_byte);
        while let Some(current) = node {
            let kind = current.kind();
            if kind.contains("interpolation") || kind.contains("substitution") {
                return false;
            }
            if kind.contains("string")
                && current.start_byte() < abs_byte
                && abs_byte < current.end_byte()
            {
                return true;
            }
            node = current.parent();
        }
        false
    }

    pub fn scope_level(&self, abs_byte: usize, kinds: &[&str]) -> Option<(usize, bool)> {
        let tree = self.tree.as_ref()?;
        let root = tree.root_node();
        let mut node = root.descendant_for_byte_range(abs_byte, abs_byte);
        let mut level = 0;
        let mut last_row = usize::MAX;
        while let Some(current) = node {
            if kinds.contains(&current.kind())
                && current.start_byte() < abs_byte
                && abs_byte < current.end_byte()
            {
                let row = current.start_position().row;
                if row != last_row {
                    level += 1;
                    last_row = row;
                }
            }
            node = current.parent();
        }
        Some((level, root.has_error()))
    }

    pub fn has_error(&self) -> bool {
        self.tree
            .as_ref()
            .map_or(true, |tree| tree.root_node().has_error())
    }

    pub fn line_of_byte(&self, abs_byte: usize) -> Option<usize> {
        self.line_starts
            .partition_point(|&start| start <= abs_byte)
            .checked_sub(1)
    }

    pub fn bracket_pair(&self, abs_byte: usize) -> Option<(usize, usize)> {
        let tree = self.tree.as_ref()?;
        let source_len = self.line_starts.last().copied().unwrap_or(0);
        let end = abs_byte.saturating_add(1).min(source_len);
        let start = abs_byte.min(end);
        let mut node = tree.root_node().descendant_for_byte_range(start, end);
        while let Some(current) = node {
            if let Some(pair) = pair_in_children(current, abs_byte) {
                return Some(pair);
            }
            node = current.parent();
        }
        None
    }

    pub fn is_code_byte(&self, abs_byte: usize) -> bool {
        let Some(tree) = self.tree.as_ref() else {
            return true;
        };
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(abs_byte, abs_byte.saturating_add(1));
        while let Some(current) = node {
            let kind = current.kind();
            if kind.contains("interpolation") || kind.contains("substitution") {
                return true;
            }
            if is_text_kind(kind)
                && current.start_byte() <= abs_byte
                && abs_byte < current.end_byte()
            {
                return false;
            }
            node = current.parent();
        }
        true
    }
}

fn capture_to_type(name: &str) -> Option<AnnotationType> {
    use AnnotationType as T;
    match name {
        "variable.builtin" => return Some(T::Builtin),
        "variable.parameter" | "parameter" => return Some(T::Parameter),
        "variable.member" => return Some(T::Property),
        "constant.numeric" => return Some(T::Number),
        "string.escape" | "escape" => return Some(T::Operator),
        "tag.attribute" => return Some(T::Attribute),
        _ => {}
    }
    match name.split('.').next()? {
        "keyword" | "conditional" | "repeat" | "include" | "exception" | "storageclass"
        | "preproc" | "define" | "tag" => Some(T::Keyword),
        "function" | "method" | "macro" => Some(T::Function),
        "type" | "constructor" => Some(T::Type),
        "module" | "namespace" | "path" => Some(T::Module),
        "property" | "field" => Some(T::Property),
        "operator" => Some(T::Operator),
        "punctuation" | "delimiter" => Some(T::Punctuation),
        "attribute" | "annotation" | "decorator" => Some(T::Attribute),
        "string" => Some(T::String),
        "comment" => Some(T::Comment),
        "number" | "float" => Some(T::Number),
        "constant" | "boolean" => Some(T::KnownValue),
        "character" => Some(T::Char),
        "label" | "lifetime" => Some(T::LifetimeSpecifier),
        _ => None,
    }
}


fn bracket_kind(kind: &str) -> Option<char> {
    match kind {
        "{" | "(" | "[" | "}" | ")" | "]" => kind.chars().next(),
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

fn pair_in_children(node: Node<'_>, at: usize) -> Option<(usize, usize)> {
    if node.child_count() == 0 {
        return None;
    }
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_missing() || child.child_count() != 0 {
            continue;
        }
        let Some(c) = bracket_kind(child.kind()) else {
            continue;
        };
        if let Some(expected) = closer_for(c) {
            stack.push((expected, child.start_byte()));
        } else if let Some(pos) = stack.iter().rposition(|(expected, _)| *expected == c) {
            let open = stack[pos].1;
            stack.truncate(pos);
            if open <= at && at <= child.start_byte() {
                return Some((open, child.start_byte()));
            }
        }
    }
    None
}

fn is_text_kind(kind: &str) -> bool {
    kind.contains("string")
        || kind.contains("comment")
        || kind.contains("char_literal")
        || kind.contains("character_literal")
        || kind == "character"
}



#[cfg(test)]
mod tests {
    use super::*;

    fn cache_for(text: &str) -> TsCache {
        let lang = super::super::highlighter::find_language("t.rs", "rs").expect("rust language");
        let ts = lang.ts.expect("rust has a parser");
        let mut cache = TsCache::new(&ts).expect("parser loads");
        let lines: Vec<Line> = text.lines().map(Line::from).collect();
        cache.update(&lines, 1);
        cache
    }

    #[test]
    fn bracket_pair_uses_the_tree() {
        let text = "fn f() { let s = \"(}\"; g(1); }";
        let cache = cache_for(text);
        assert!(!cache.has_error());

        let one = text.find('1').unwrap();
        let open = text.find("g(").unwrap() + 1;
        assert_eq!(cache.bracket_pair(one), Some((open, one + 1)));

        let in_string = text.find("\"(}\"").unwrap() + 1;
        assert_eq!(
            cache.bracket_pair(in_string),
            Some((text.find('{').unwrap(), text.rfind('}').unwrap()))
        );
    }

    #[test]
    fn code_vs_string_and_comment_bytes() {
        let text = "fn f() {} // (";
        let cache = cache_for(text);
        assert!(cache.is_code_byte(0));
        assert!(!cache.is_code_byte(text.rfind('(').unwrap()));

        let text = "fn f() { let s = \"(\"; }";
        let cache = cache_for(text);
        assert!(!cache.is_code_byte(text.find("\"(\"").unwrap() + 1));
        assert!(cache.is_code_byte(text.find("let").unwrap()));
    }

    #[test]
    fn syntax_errors_are_detected() {
        assert!(cache_for("fn f( {").has_error());
    }

    #[test]
    fn lines_map_to_bytes() {
        let cache = cache_for("ab\ncd\n");
        assert_eq!(cache.line_start(1), Some(3));
        assert_eq!(cache.line_of_byte(4), Some(1));
        assert!(cache.is_current(1));
        assert!(!cache.is_current(2));
    }
}
