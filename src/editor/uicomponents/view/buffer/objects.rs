#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use super::{word_class, Buffer};
use crate::editor::command::Obj;
use crate::prelude::*;

pub struct ObjRange {
    pub start: Location,
    pub end: Location,
    pub linewise: bool,
}

fn at(line_idx: usize, grapheme_idx: usize) -> Location {
    Location { line_idx, grapheme_idx }
}

impl Buffer {
    pub fn text_object(&self, obj: Obj, around: bool, cursor: Location) -> Option<ObjRange> {
        match obj {
            Obj::Word => self.word_object(cursor, around, false),
            Obj::BigWord => self.word_object(cursor, around, true),
            Obj::Quote(q) => self.quote_object(cursor, q, around),
            Obj::Pair(open) => self.pair_object(cursor, open, around),
            Obj::Paragraph => self.paragraph_object(cursor, around),
        }
    }

    fn word_object(&self, cur: Location, around: bool, big: bool) -> Option<ObjRange> {
        let line = self.lines.get(cur.line_idx)?;
        let count = line.grapheme_count();
        if count == 0 {
            return None;
        }
        let cls = |i: usize| word_class(line.grapheme_at(i).unwrap_or(" "), big);
        let g = cur.grapheme_idx.min(count - 1);
        let c = cls(g);
        let mut s = g;
        while s > 0 && cls(s - 1) == c {
            s -= 1;
        }
        let mut e = g + 1;
        while e < count && cls(e) == c {
            e += 1;
        }
        if around {
            if c != 0 {
                let mut t = e;
                while t < count && cls(t) == 0 {
                    t += 1;
                }
                if t > e {
                    e = t;
                } else {
                    while s > 0 && cls(s - 1) == 0 {
                        s -= 1;
                    }
                }
            } else if e < count {
                let c2 = cls(e);
                while e < count && cls(e) == c2 {
                    e += 1;
                }
            }
        }
        Some(ObjRange { start: at(cur.line_idx, s), end: at(cur.line_idx, e), linewise: false })
    }

    fn quote_object(&self, cur: Location, quote: char, around: bool) -> Option<ObjRange> {
        let line = self.lines.get(cur.line_idx)?;
        let count = line.grapheme_count();
        let is_quote = |i: usize| {
            line.grapheme_at(i).is_some_and(|s| s.chars().eq(std::iter::once(quote)))
        };
        let mut marks = Vec::new();
        for i in 0..count {
            if is_quote(i) && !(i > 0 && line.grapheme_at(i - 1) == Some("\\")) {
                marks.push(i);
            }
        }
        let g = cur.grapheme_idx;
        let pair = marks
            .chunks_exact(2)
            .find(|p| p[0] <= g && g <= p[1])
            .or_else(|| marks.chunks_exact(2).find(|p| p[0] > g))?;
        let (mut s, mut e) = (pair[0], pair[1] + 1);
        if around {
            let blank = |i: usize| line.grapheme_at(i).is_some_and(|x| x.trim().is_empty());
            let mut t = e;
            while t < count && blank(t) {
                t += 1;
            }
            if t > e {
                e = t;
            } else {
                while s > 0 && blank(s - 1) {
                    s -= 1;
                }
            }
        } else {
            s += 1;
            e -= 1;
        }
        Some(ObjRange { start: at(cur.line_idx, s), end: at(cur.line_idx, e), linewise: false })
    }

    fn pair_object(&self, cur: Location, open: char, around: bool) -> Option<ObjRange> {
        let (o, c) = self.pair_of_kind(cur, open)?;
        if around {
            return Some(ObjRange {
                start: o,
                end: at(c.line_idx, c.grapheme_idx + 1),
                linewise: false,
            });
        }
        let opener_ends_line = o.grapheme_idx + 1 >= self.grapheme_count(o.line_idx);
        let closer_alone = self.lines.get(c.line_idx).is_some_and(|l| {
            l.chars().take(l.len()).take_while(|ch| ch.is_whitespace()).count()
                >= self.grapheme_count(c.line_idx).min(c.grapheme_idx)
        });
        if o.line_idx < c.line_idx && opener_ends_line && closer_alone {
            let (first, last) = (o.line_idx + 1, c.line_idx - 1);
            if first <= last {
                return Some(ObjRange { start: at(first, 0), end: at(last, 0), linewise: true });
            }
        }
        Some(ObjRange { start: at(o.line_idx, o.grapheme_idx + 1), end: c, linewise: false })
    }

    fn paragraph_object(&self, cur: Location, around: bool) -> Option<ObjRange> {
        let last = self.lines.len().checked_sub(1)?;
        let l = cur.line_idx.min(last);
        let blank = self.is_blank_line(l);
        let (mut s, mut e) = (l, l);
        while s > 0 && self.is_blank_line(s - 1) == blank {
            s -= 1;
        }
        while e < last && self.is_blank_line(e + 1) == blank {
            e += 1;
        }
        if around {
            if e < last {
                let next = self.is_blank_line(e + 1);
                e += 1;
                while e < last && self.is_blank_line(e + 1) == next {
                    e += 1;
                }
            } else if s > 0 {
                let prev = self.is_blank_line(s - 1);
                s -= 1;
                while s > 0 && self.is_blank_line(s - 1) == prev {
                    s -= 1;
                }
            }
        }
        Some(ObjRange { start: at(s, 0), end: at(e, 0), linewise: true })
    }
}