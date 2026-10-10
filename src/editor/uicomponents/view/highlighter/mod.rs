use super::super::super::{Annotation, AnnotationType, Line};
use crate::prelude::*;
use std::sync::Arc;
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
pub use registry::{find_language, reload_plugins};

enum SyntaxSource<'a> {
    Generic(Box<dyn SyntaxHighlighter>),
    Tree(&'a [Vec<Annotation>]),
}

#[derive(Default)]
pub struct Highlighter<'a> {
    syntax: Option<SyntaxSource<'a>>,
    search_result_highlighter: Option<SearchResultHighlighter<'a>>,
    selection: Option<(Location, Location)>,
    scope: Option<(Location, Location)>,
    brackets: Option<(Location, Location)>,
}

impl<'a> Highlighter<'a> {
    pub fn new(
        matched_word: Option<&'a str>,
        selected_match: Option<Location>,
        language: Option<Arc<Language>>,
        tree_annotations: Option<&'a [Vec<Annotation>]>,
        selection: Option<(Location, Location)>,
        scope: Option<(Location, Location)>,
        brackets: Option<(Location, Location)>,
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
            scope,
            brackets,
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
        if let Some((open, close)) = self.scope {
            if idx == open.line_idx || idx == close.line_idx {
                let g_start = if idx == open.line_idx {
                    open.grapheme_idx.saturating_add(1)
                } else {
                    0
                };
                let g_end = if idx == close.line_idx {
                    close.grapheme_idx
                } else {
                    line.grapheme_count()
                };
                let byte_range = line.byte_range_for_grapheme_range(g_start..g_end);
                if byte_range.start < byte_range.end {
                    result.push(Annotation {
                        annotation_type: AnnotationType::Scope,
                        start: byte_range.start,
                        end: byte_range.end,
                    });
                }
            }
        }
        if let Some((open, close)) = self.brackets {
            for at in [open, close] {
                if at.line_idx == idx {
                    let byte_range = line.byte_range_for_grapheme_range(
                        at.grapheme_idx..at.grapheme_idx.saturating_add(1),
                    );
                    if byte_range.start < byte_range.end {
                        result.push(Annotation {
                            annotation_type: AnnotationType::Bracket,
                            start: byte_range.start,
                            end: byte_range.end,
                        });
                    }
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







#[cfg(test)]
mod tests {
    use super::*;

    fn loc(line_idx: usize, grapheme_idx: usize) -> Location {
        Location { line_idx, grapheme_idx }
    }

    fn kinds(list: &[Annotation], kind: AnnotationType) -> Vec<(usize, usize)> {
        list.iter()
            .filter(|a| a.annotation_type == kind)
            .map(|a| (a.start, a.end))
            .collect()
    }

    #[test]
    fn selection_is_annotated_per_line() {
        let h = Highlighter::new(None, None, None, None, Some((loc(0, 1), loc(1, 2))), None, None);
        let first = h.get_annotations(0, &Line::from("abcdef"));
        assert_eq!(kinds(&first, AnnotationType::Selection), vec![(1, 6)]);
        let second = h.get_annotations(1, &Line::from("xyz"));
        assert_eq!(kinds(&second, AnnotationType::Selection), vec![(0, 2)]);
        assert!(h.get_annotations(2, &Line::from("zzz")).is_empty());
    }

    #[test]
    fn scope_tint_covers_the_inside_of_the_pair() {
        let scope = Some((loc(0, 1), loc(1, 2)));
        let h = Highlighter::new(None, None, None, None, None, scope, None);
        let open = h.get_annotations(0, &Line::from("x{ab"));
        assert_eq!(kinds(&open, AnnotationType::Scope), vec![(2, 4)]);
        let close = h.get_annotations(1, &Line::from("cd}y"));
        assert_eq!(kinds(&close, AnnotationType::Scope), vec![(0, 2)]);
        assert!(h.get_annotations(2, &Line::from("zz")).is_empty());
    }

    #[test]
    fn bracket_highlight_marks_both_characters() {
        let pair = Some((loc(0, 1), loc(1, 2)));
        let h = Highlighter::new(None, None, None, None, None, None, pair);
        let open = h.get_annotations(0, &Line::from("x{ab"));
        assert_eq!(kinds(&open, AnnotationType::Bracket), vec![(1, 2)]);
        let close = h.get_annotations(1, &Line::from("cd}y"));
        assert_eq!(kinds(&close, AnnotationType::Bracket), vec![(2, 3)]);
    }

    #[test]
    fn nothing_enabled_means_no_line_pass() {
        let h = Highlighter::new(None, None, None, None, None, None, None);
        assert!(!h.needs_line_pass());
    }
}
