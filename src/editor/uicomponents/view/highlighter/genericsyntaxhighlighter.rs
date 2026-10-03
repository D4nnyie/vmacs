use super::common::{annotate_char, annotate_lifetime_specifier, annotate_next_word, annotate_number};
use super::language::Language;
use super::{Annotation, AnnotationType, Line, SyntaxHighlighter};
use crate::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

pub struct GenericSyntaxHighlighter {
    lang: &'static Language,
    highlights: Vec<Vec<Annotation>>,
    ml_comment_balance: usize,
    open_string: Option<usize>,
}

impl GenericSyntaxHighlighter {
    pub fn new(lang: &'static Language) -> Self {
        Self {
            lang,
            highlights: Vec::new(),
            ml_comment_balance: 0,
            open_string: None,
        }
    }

    fn annotate_ml_comment(&mut self, string: &str) -> Option<Annotation> {
        let (open, close) = self.lang.config.block_comment?;
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
        let delimiters = self.lang.config.string_delimiters;

        if let Some(idx) = self.open_string {
            let Some(&(open, close)) = delimiters.get(idx) else {
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

        let mut best: Option<(usize, &'static str, &'static str)> = None;
        for (i, &(open, close)) in delimiters.iter().enumerate() {
            if !open.is_empty()
                && string.starts_with(open)
                && best.map_or(true, |(_, best_open, _)| open.len() > best_open.len())
            {
                best = Some((i, open, close));
            }
        }
        let (idx, open, close) = best?;

        let (end, closed) = Self::find_close(string, open.len(), close, self.has_escapes(open));
        if !closed && self.lang.multiline_strings.iter().any(|m| *m == open) {
            self.open_string = Some(idx);
        }
        Some(Annotation {
            annotation_type: AnnotationType::String,
            start: 0,
            end,
        })
    }

    fn annotate_line_comment(&self, string: &str) -> Option<Annotation> {
        let prefix = self.lang.config.line_comment?;
        string.starts_with(prefix).then_some(Annotation {
            annotation_type: AnnotationType::Comment,
            start: 0,
            end: string.len(),
        })
    }

    fn contains(list: &[&str], word: &str, case_insensitive: bool) -> bool {
        list.iter().any(|candidate| {
            if case_insensitive {
                candidate.eq_ignore_ascii_case(word)
            } else {
                *candidate == word
            }
        })
    }

    fn annotate_keyword(&self, string: &str) -> Option<Annotation> {
        let list = self.lang.config.keywords;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::Keyword, |w| Self::contains(list, w, ci))
    }
    fn annotate_type(&self, string: &str) -> Option<Annotation> {
        let list = self.lang.config.types;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::Type, |w| Self::contains(list, w, ci))
    }
    fn annotate_known_value(&self, string: &str) -> Option<Annotation> {
        let list = self.lang.config.known_values;
        let ci = self.lang.case_insensitive;
        annotate_next_word(string, AnnotationType::KnownValue, |w| Self::contains(list, w, ci))
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
