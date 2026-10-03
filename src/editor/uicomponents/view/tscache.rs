#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
//! Tree-sitter parse tree + highlight captures for one buffer.
//!
//! The whole buffer is reparsed whenever `Buffer::revision` changes (typically a
//! few ms). The result is converted to per-line, line-relative byte annotations,
//! which is exactly what `AnnotatedString` consumes.

use super::super::super::{Annotation, AnnotationType, Line};
use super::highlighter::TsConfig;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

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

        raw.sort_by_key(|&(start, end, _)| (start, std::cmp::Reverse(end)));

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

    /// Number of scope nodes (kinds in `kinds`) that strictly contain `abs_byte`,
    /// counting at most one per starting row. Also returns whether the tree has
    /// syntax errors (then the caller should trust the heuristic more).
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
}

fn capture_to_type(name: &str) -> Option<AnnotationType> {
    let head = name.split('.').next()?;
    match head {
        "keyword" | "conditional" | "repeat" | "include" | "exception" | "storageclass" => {
            Some(AnnotationType::Keyword)
        }
        "variable" if name == "variable.builtin" => Some(AnnotationType::Keyword),
        "type" | "constructor" => Some(AnnotationType::Type),
        "string" => Some(AnnotationType::String),
        "comment" => Some(AnnotationType::Comment),
        "number" | "float" => Some(AnnotationType::Number),
        "constant" | "boolean" => {
            if name == "constant.numeric" {
                Some(AnnotationType::Number)
            } else {
                Some(AnnotationType::KnownValue)
            }
        }
        "character" => Some(AnnotationType::Char),
        "label" | "lifetime" => Some(AnnotationType::LifetimeSpecifier),
        _ => None,
    }
}
