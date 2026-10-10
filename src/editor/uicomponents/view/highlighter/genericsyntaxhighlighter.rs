use super::common::{annotate_char, annotate_lifetime_specifier, annotate_next_word, annotate_number};
use super::language::Language;
use super::{Annotation, AnnotationType, Line, SyntaxHighlighter};
use std::collections::HashSet;
use std::sync::Arc;
use crate::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

pub struct GenericSyntaxHighlighter {
    lang: Arc<Language>,
    highlights: Vec<Vec<Annotation>>,
    ml_comment_balance: usize,
    open_string: Option<usize>,
}

impl GenericSyntaxHighlighter {
    pub fn new(lang: Arc<Language>) -> Self {
        Self {
            lang,
            highlights: Vec::new(),
            ml_comment_balance: 0,
            open_string: None,
        }
    }

    fn annotate_ml_comment(&mut self, string: &str) -> Option<Annotation> {
        let lang = Arc::clone(&self.lang);
        let (open, close) = lang.config.block_comment.as_ref()?;
        let (open, close) = (open.as_str(), close.as_str());
        let char_indices: Vec<(usize, char)> = string.char_indices().collect();
        let mut i = 0;
        while i < char_indices.len() {
            let byte_idx = char_indices[i].0;
            let remainder = &string[byte_idx..];
            if remainder.starts_with(open) {
                self.ml_comment_balance = self.ml_comment_balance.saturating_add(1);
                i = i.saturating_add(open.chars().count());
                continue;
            } else if self.ml_comment_balance == 0 {
                return None;
            } else if remainder.starts_with(close) {
                self.ml_comment_balance = self.ml_comment_balance.saturating_sub(1);
                if self.ml_comment_balance == 0 {
                    let end = byte_idx.saturating_add(close.len());
                    return Some(Annotation {
                        annotation_type: AnnotationType::Comment,
                        start: 0,
                        end,
                    });
                }
                i = i.saturating_add(close.chars().count());
                continue;
            }
            i = i.saturating_add(1);
        }
        (self.ml_comment_balance > 0).then_some(Annotation {
            annotation_type: AnnotationType::Comment,
            start: 0,
            end: string.len(),
        })
    }

    fn has_escapes(&self, open: &str) -> bool {
        !(open.starts_with('r')
            || open.starts_with('R')
            || open == "[["
            || (open == "`" && self.lang.config.name == "Go"))
    }

    fn find_close(string: &str, from: usize, close: &str, escapes: bool) -> (usize, bool) {
        let mut chars = string.get(from..).unwrap_or("").char_indices();
        while let Some((i, c)) = chars.next() {
            let idx = from.saturating_add(i);
            if escapes && c == '\\' {
                chars.next();
                continue;
            }
            if string.get(idx..).is_some_and(|rest| rest.starts_with(close)) {
                return (idx.saturating_add(close.len()), true);
            }
        }
        (string.len(), false)
    }

    fn annotate_string(&mut self, string: &str) -> Option<Annotation> {
        let lang = Arc::clone(&self.lang);
        let delimiters = &lang.config.string_delimiters;

        if let Some(idx) = self.open_string {
            let Some((open, close)) = delimiters.get(idx) else {
                self.open_string = None;
                return None;
            };
            let (end, closed) = Self::find_close(string, 0, close, self.has_escapes(open));
            if closed {
                self.open_string = None;
            }
            return Some(Annotation {
                annotation_type: AnnotationType::String,
                start: 0,
                end,
            });
        }

        let mut best: Option<(usize, &str, &str)> = None;
        for (i, (open, close)) in delimiters.iter().enumerate() {
            if !open.is_empty()
                && string.starts_with(open.as_str())
                && best.map_or(true, |(_, best_open, _)| open.len() > best_open.len())
            {
                best = Some((i, open.as_str(), close.as_str()));
            }
        }
        let (idx, open, close) = best?;

        let (end, closed) = Self::find_close(string, open.len(), close, self.has_escapes(open));
        if !closed && self.lang.multiline_strings.iter().any(|m| m.as_str() == open) {
            self.open_string = Some(idx);
        }
        Some(Annotation {
            annotation_type: AnnotationType::String,
            start: 0,
            end,
        })
    }

    fn annotate_line_comment(&self, string: &str) -> Option<Annotation> {
        let prefix = self.lang.config.line_comment.as_deref()?;
        string.starts_with(prefix).then_some(Annotation {
            annotation_type: AnnotationType::Comment,
            start: 0,
            end: string.len(),
        })
    }

    fn contains(set: &HashSet<String>, word: &str, case_insensitive: bool) -> bool {
        if case_insensitive {
            set.contains(&word.to_ascii_lowercase())
        } else {
            set.contains(word)
        }
    }

    fn annotate_keyword(&self, string: &str) -> Option<Annotation> {
        let set = &self.lang.words.keywords;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::Keyword, |w| Self::contains(set, w, ci))
    }
    fn annotate_type(&self, string: &str) -> Option<Annotation> {
        let set = &self.lang.words.types;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::Type, |w| Self::contains(set, w, ci))
    }
    fn annotate_known_value(&self, string: &str) -> Option<Annotation> {
        let set = &self.lang.words.known_values;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::KnownValue, |w| Self::contains(set, w, ci))
    }

    fn initial_annotation(&mut self, line: &Line) -> Option<Annotation> {
        if self.open_string.is_some() {
            self.annotate_string(line)
        } else if self.ml_comment_balance > 0 {
            self.annotate_ml_comment(line)
        } else {
            None
        }
    }

    fn annotate_remainder(&mut self, remainder: &str) -> Option<Annotation> {
        let supports_char = self.lang.config.supports_char_literal;
        let supports_lifetime = self.lang.config.supports_lifetime;

        self.annotate_ml_comment(remainder)
            .or_else(|| self.annotate_string(remainder))
            .or_else(|| self.annotate_line_comment(remainder))
            .or_else(|| supports_char.then(|| annotate_char(remainder)).flatten())
            .or_else(|| supports_lifetime.then(|| annotate_lifetime_specifier(remainder)).flatten())
            .or_else(|| annotate_number(remainder))
            .or_else(|| self.annotate_keyword(remainder))
            .or_else(|| self.annotate_type(remainder))
            .or_else(|| self.annotate_known_value(remainder))
    }
}

impl SyntaxHighlighter for GenericSyntaxHighlighter {
    fn highlight(&mut self, idx: LineIdx, line: &Line) {
        debug_assert_eq!(idx, self.highlights.len());
        let mut result = Vec::new();
        let mut iterator = line.split_word_bound_indices().peekable();
        if let Some(annotation) = self.initial_annotation(line) {
            result.push(annotation);
            while let Some(&(next_idx, _)) = iterator.peek() {
                if next_idx >= annotation.end {
                    break;
                }
                iterator.next();
            }
        }
        while let Some((start_idx, _)) = iterator.next() {
            let remainder = &line[start_idx..];
            if let Some(mut annotation) = self.annotate_remainder(remainder) {
                annotation.shift(start_idx);
                result.push(annotation);
                while let Some(&(next_idx, _)) = iterator.peek() {
                    if next_idx >= annotation.end {
                        break;
                    }
                    iterator.next();
                }
            };
        }
        self.highlights.push(result);
    }

    fn get_annotations(&self, idx: LineIdx) -> Option<&Vec<Annotation>> {
        self.highlights.get(idx)
    }
}












#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::AnnotationType;

    fn highlight_all(file: &str, ext: &str, lines: &[&str]) -> GenericSyntaxHighlighter {
        let lang = super::super::find_language(file, ext).expect("builtin language");
        let mut highlighter = GenericSyntaxHighlighter::new(lang);
        for (idx, text) in lines.iter().enumerate() {
            highlighter.highlight(idx, &Line::from(text));
        }
        highlighter
    }

    fn has(h: &GenericSyntaxHighlighter, idx: usize, kind: AnnotationType, start: usize, end: usize) -> bool {
        h.get_annotations(idx).is_some_and(|list| {
            list.iter()
                .any(|a| a.annotation_type == kind && a.start == start && a.end == end)
        })
    }

    #[test]
    fn keywords_numbers_and_line_comments() {
        let h = highlight_all("a.cs", "cs", &["int x = 5; // hi"]);
        assert!(has(&h, 0, AnnotationType::Keyword, 0, 3));
        assert!(has(&h, 0, AnnotationType::Number, 8, 9));
        assert!(has(&h, 0, AnnotationType::Comment, 11, 16));
    }

    #[test]
    fn strings_are_not_scanned_for_keywords() {
        let h = highlight_all("a.cs", "cs", &["x = \"int\";"]);
        assert!(has(&h, 0, AnnotationType::String, 4, 9));
        assert!(!has(&h, 0, AnnotationType::Keyword, 5, 8));
    }

    #[test]
    fn multi_line_strings_carry_over() {
        let h = highlight_all("a.py", "py", &["x = \"\"\"abc", "def\"\"\"", "y"]);
        assert!(has(&h, 0, AnnotationType::String, 4, 10));
        assert!(has(&h, 1, AnnotationType::String, 0, 6));
        let last = h.get_annotations(2).unwrap();
        assert!(last.iter().all(|a| a.annotation_type != AnnotationType::String));
    }

    #[test]
    fn single_line_block_comment() {
        let h = highlight_all("a.lua", "lua", &["x = 1 --[[ c ]] y"]);
        assert!(has(&h, 0, AnnotationType::Comment, 6, 15));
    }

    #[test]
    fn case_insensitive_languages_match_any_case() {
        let h = highlight_all("q.sql", "sql", &["select 1", "SeLeCt 2"]);
        assert!(has(&h, 0, AnnotationType::Keyword, 0, 6));
        assert!(has(&h, 1, AnnotationType::Keyword, 0, 6));
    }
}
