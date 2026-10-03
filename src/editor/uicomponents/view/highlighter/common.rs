use super::{Annotation, AnnotationType};
use unicode_segmentation::UnicodeSegmentation;

pub fn annotate_next_word<F>(string: &str, annotation_type: AnnotationType, validator: F) -> Option<Annotation>
where
    F: Fn(&str) -> bool,
{
    if let Some(word) = string.split_word_bounds().next() {
        if validator(word) {
            return Some(Annotation { annotation_type, start: 0, end: word.len() });
        }
    }
    None
}

pub fn annotate_number(string: &str) -> Option<Annotation> {
    annotate_next_word(string, AnnotationType::Number, is_valid_number)
}

pub fn annotate_char(string: &str) -> Option<Annotation> {
    let mut iter = string.split_word_bound_indices().peekable();
    if let Some((_, "\'")) = iter.next() {
        if let Some((_, "\\")) = iter.peek() {
            iter.next();
        }
        iter.next();
        if let Some((idx, "\'")) = iter.next() {
            return Some(Annotation {
                annotation_type: AnnotationType::Char,
                start: 0,
                end: idx.saturating_add(1),
            });
        }
    }
    None
}

pub fn annotate_lifetime_specifier(string: &str) -> Option<Annotation> {
    let mut iter = string.split_word_bound_indices();
    if let Some((_, "\'")) = iter.next() {
        if let Some((idx, next_word)) = iter.next() {
            return Some(Annotation {
                annotation_type: AnnotationType::LifetimeSpecifier,
                start: 0,
                end: idx.saturating_add(next_word.len()),
            });
        }
    }
    None
}

fn is_valid_number(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    if is_numeric_literal(word) {
        return true;
    }
    let mut chars = word.chars();
    if let Some(first_char) = chars.next() {
        if !first_char.is_ascii_digit() {
            return false;
        }
    }
    let mut seen_dot = false;
    let mut seen_e = false;
    let mut prev_was_digit = true;
    for char in chars {
        match char {
            '0'..='9' => prev_was_digit = true,
            '_' => {
                if !prev_was_digit { return false; }
                prev_was_digit = false;
            }
            '.' => {
                if seen_dot || seen_e || !prev_was_digit { return false; }
                seen_dot = true;
                prev_was_digit = false;
            }
            'e' | 'E' => {
                if seen_e || !prev_was_digit { return false; }
                seen_e = true;
                prev_was_digit = false;
            }
            _ => return false,
        }
    }
    prev_was_digit
}

fn is_numeric_literal(word: &str) -> bool {
    if word.len() < 3 {
        return false;
    }
    let mut chars = word.chars();
    if chars.next() != Some('0') {
        return false;
    }
    let base = match chars.next() {
        Some('b' | 'B') => 2,
        Some('o' | 'O') => 8,
        Some('x' | 'X') => 16,
        _ => return false,
    };
    chars.all(|char| char.is_digit(base))
}