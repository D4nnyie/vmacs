use super::super::super::{Annotation, AnnotationType, Line};
use crate::prelude::*;
mod syntaxhighlighter;
use searchresulthighlighter::SearchResultHighlighter;
use syntaxhighlighter::SyntaxHighlighter;
mod common;
mod genericsyntaxhighlighter;
mod language;
mod registry;
mod searchresulthighlighter;
use genericsyntaxhighlighter::GenericSyntaxHighlighter;
pub use language::{Language, TsConfig};
pub use registry::find_by_extension;

enum SyntaxSource<'a> {
    Generic(Box<dyn SyntaxHighlighter>),
    Tree(&'a [Vec<Annotation>]),
}

#[derive(Default)]
pub struct Highlighter<'a> {
    syntax: Option<SyntaxSource<'a>>,
    search_result_highlighter: Option<SearchResultHighlighter<'a>>,
    selection: Option<(Location, Location)>,
}

impl<'a> Highlighter<'a> {
    pub fn new(
        matched_word: Option<&'a str>,
        selected_match: Option<Location>,
        language: Option<&'static Language>,
        tree_annotations: Option<&'a [Vec<Annotation>]>,
        selection: Option<(Location, Location)>,
    ) -> Self {
        let search_result_highlighter = matched_word
            .map(|matched_word| SearchResultHighlighter::new(matched_word, selected_match));
        let syntax = match (tree_annotations, language) {
            (Some(tree), _) => Some(SyntaxSource::Tree(tree)),
            (None, Some(lang)) => Some(SyntaxSource::Generic(Box::new(
                GenericSyntaxHighlighter::new(lang),
            ))),
            (None, None) => None,
        };
        Self {
            syntax,
            search_result_highlighter,
            selection,
        }
    }

    pub fn needs_line_pass(&self) -> bool {
        matches!(self.syntax, Some(SyntaxSource::Generic(_)))
            || self.search_result_highlighter.is_some()
    }

    pub fn get_annotations(&self, idx: LineIdx, line: &Line) -> Vec<Annotation> {
        let mut result = Vec::new();
        match &self.syntax {
            Some(SyntaxSource::Generic(highlighter)) => {
                if let Some(annotations) = highlighter.get_annotations(idx) {
                    result.extend(annotations.iter().copied());
                }
            }
            Some(SyntaxSource::Tree(per_line)) => {
                if let Some(annotations) = per_line.get(idx) {
                    result.extend(annotations.iter().copied());
                }
            }
            None => {}
        }
        if let Some(search_result_highlighter) = &self.search_result_highlighter {
            if let Some(annotations) = search_result_highlighter.get_annotations(idx) {
                result.extend(annotations.iter().copied());
            }
        }
        if let Some((start, end)) = self.selection {
            if idx >= start.line_idx && idx <= end.line_idx {
                let g_start = if idx == start.line_idx { start.grapheme_idx } else { 0 };
                let g_end = if idx == end.line_idx { end.grapheme_idx } else { line.grapheme_count() };
                let byte_range = line.byte_range_for_grapheme_range(g_start..g_end);
                if byte_range.start < byte_range.end {
                    result.push(Annotation {
                        annotation_type: AnnotationType::Selection,
                        start: byte_range.start,
                        end: byte_range.end,
                    });
                }
            }
        }
        result
    }

    pub fn highlight(&mut self, idx: LineIdx, line: &Line) {
        if let Some(SyntaxSource::Generic(highlighter)) = &mut self.syntax {
            highlighter.highlight(idx, line);
        }
        if let Some(search_result_highlighter) = &mut self.search_result_highlighter {
            search_result_highlighter.highlight(idx, line);
        }
    }
}
