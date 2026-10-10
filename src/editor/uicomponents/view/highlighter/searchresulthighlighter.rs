use std::collections::HashMap;

use super::{syntaxhighlighter::SyntaxHighlighter, Annotation, AnnotationType, Line};
use crate::prelude::*;

#[derive(Default)]
pub struct SearchResultHighlighter<'a> {
    matched_word: &'a str,
    selected_match: Option<Location>,
    highlights: HashMap<LineIdx, Vec<Annotation>>,
}

impl<'a> SearchResultHighlighter<'a> {
    pub fn new(matched_word: &'a str, selected_match: Option<Location>) -> Self {
        Self {
            matched_word,
            selected_match,
            highlights: HashMap::new(),
        }
    }

    fn highlight_matched_words(&self, line: &Line, result: &mut Vec<Annotation>) {
        if self.matched_word.is_empty() {
            return;
        }
        line.find_all(self.matched_word, 0..line.len())
            .iter()
            .for_each(|(start, _)| {
                result.push(Annotation {
                    annotation_type: AnnotationType::Match,
                    start: *start,
                    end: start.saturating_add(self.matched_word.len()),
                });
            });
    }
    /*
    fn highlight_selected_match(&self, result: &mut Vec<Annotation>) {
        if let Some(selected_match) = self.selected_match {
            if self.matched_word.is_empty() {
                return;
            }
            let start = selected_match.grapheme_idx;
            result.push(Annotation {
                annotation_type: AnnotationType::SelectedMatch,
                start,
                end: start.saturating_add(self.matched_word.len()),
            });
        }
    }
    */
    fn highlight_selected_match(
        &self,
        line: &Line,
        selected: Location,
        result: &mut Vec<Annotation>,
    ) {
        if self.matched_word.is_empty() {
            return;
        }
        // Only highlight if the query really matches at the cursor.
        let found = line
            .find_all(self.matched_word, 0..line.len())
            .into_iter()
            .find(|(_, grapheme_idx)| *grapheme_idx == selected.grapheme_idx);
        if let Some((start, _)) = found {
            result.push(Annotation {
                annotation_type: AnnotationType::SelectedMatch,
                start,
                end: start.saturating_add(self.matched_word.len()),
            });
        }
    }
}

impl<'a> SyntaxHighlighter for SearchResultHighlighter<'a> {
    fn highlight(&mut self, idx: LineIdx, line: &Line) {
        let mut result = Vec::new();
        self.highlight_matched_words(line, &mut result);
        if let Some(selected_match) = self.selected_match {
            if selected_match.line_idx == idx {
                self.highlight_selected_match(line, selected_match, &mut result);
            }
        }
        self.highlights.insert(idx, result);
    }
    fn get_annotations(&self, idx: LineIdx) -> Option<&Vec<Annotation>> {
        self.highlights.get(&idx)
    }
}





#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_every_match_and_the_selected_one() {
        let mut h = SearchResultHighlighter::new(
            "ab",
            Some(Location { line_idx: 0, grapheme_idx: 3 }),
        );
        h.highlight(0, &Line::from("ab ab"));
        let list = h.get_annotations(0).unwrap();
        let matches: Vec<_> = list
            .iter()
            .filter(|a| a.annotation_type == AnnotationType::Match)
            .map(|a| a.start)
            .collect();
        assert_eq!(matches, vec![0, 3]);
        let selected: Vec<_> = list
            .iter()
            .filter(|a| a.annotation_type == AnnotationType::SelectedMatch)
            .map(|a| a.start)
            .collect();
        assert_eq!(selected, vec![3]);
    }

    #[test]
    fn empty_query_highlights_nothing() {
        let mut h = SearchResultHighlighter::new("", None);
        h.highlight(0, &Line::from("abc"));
        assert!(h.get_annotations(0).unwrap().is_empty());
    }
}