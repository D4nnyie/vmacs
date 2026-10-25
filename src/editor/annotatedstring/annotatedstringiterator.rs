use crate::prelude::*;
//use std::cmp::min;
use super::AnnotationType;

use super::{AnnotatedString, AnnotatedStringPart};

pub struct AnnotatedStringIterator<'a> {
    pub annotated_string: &'a AnnotatedString,
    pub current_idx: ByteIdx,
}

impl<'a> Iterator for AnnotatedStringIterator<'a> {
    type Item = AnnotatedStringPart<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let len = self.annotated_string.string.len();
        if self.current_idx >= len {
            return None;
        }
        let current = self.current_idx;
        let mut end_idx = len;
        let mut winner = None;
        let mut scoped = false;
        for annotation in &self.annotated_string.annotations {
            if annotation.start <= current && annotation.end > current {
                end_idx = end_idx.min(annotation.end);
                if annotation.annotation_type == AnnotationType::Scope {
                    scoped = true;
                } else {
                    winner = Some(annotation.annotation_type);
                }
            } else if annotation.start > current {
                end_idx = end_idx.min(annotation.start);
            }
        }
        self.current_idx = end_idx;
        Some(AnnotatedStringPart {
            string: &self.annotated_string.string[current..end_idx],
            annotation_type: winner,
            scoped,
        })
    }
}