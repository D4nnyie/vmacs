use super::super::folds::{attach_brace_lines, brace_folds, indent_folds, is_closing_line, Fold, Hidden};
use super::Buffer;
use std::time::Instant;

const FOLD_ANIM_MS: u128 = 140;

#[derive(Clone, Copy)]
pub(super) struct FoldAnim {
    start: usize,
    end: usize,
    began: Instant,
    closing: bool,
}

impl Buffer {
    pub fn fold_state(&self, line: usize) -> Option<bool> {
        if self.closed.contains(&line) {
            Some(true)
        } else {
            self.fold_at(line).map(|_| false)
        }
    }

    pub fn enclosing_fold_starts(&self, line: usize) -> Vec<usize> {
        self.containing(line).into_iter().map(|f| f.start).collect()
    }

    pub(super) fn refresh_folds(&mut self) {
        if self.folds_rev == Some(self.revision) {
            return;
        }
        self.folds_rev = Some(self.revision);
        let has_parser = self
            .file_info
            .get_language()
            .is_some_and(|lang| lang.ts.is_some());
        let tree = self
            .ts_cache
            .as_ref()
            .filter(|cache| cache.is_current(self.revision));
        self.folds = match tree {
            Some(cache) if has_parser && !cache.has_error() => cache.folds().to_vec(),
            _ => {
                let braces = attach_brace_lines(brace_folds(&self.lines), &self.lines);
                if braces.is_empty() {
                    attach_brace_lines(indent_folds(&self.lines), &self.lines)
                } else {
                    braces
                }
            }
        };
        if !self.closed.is_empty() {
            let folds = &self.folds;
            self.closed
                .retain(|start| folds.binary_search_by_key(start, |f| f.start).is_ok());
        }
    }

    pub(super) fn shift_closed(&mut self, idx: usize, inserted: bool) {
        if self.closed.is_empty() {
            return;
        }
        let old = std::mem::take(&mut self.closed);
        self.closed = old
            .into_iter()
            .filter_map(|start| {
                if inserted {
                    Some(if start >= idx { start.saturating_add(1) } else { start })
                } else if start == idx {
                    None
                } else if start > idx {
                    Some(start.saturating_sub(1))
                } else {
                    Some(start)
                }
            })
            .collect();
    }

    fn fold_at(&self, start: usize) -> Option<Fold> {
        self.folds
            .binary_search_by_key(&start, |f| f.start)
            .ok()
            .and_then(|i| self.folds.get(i).copied())
    }

    fn containing(&self, line: usize) -> Vec<Fold> {
        self.folds
            .iter()
            .rev()
            .filter(|f| {
                f.start <= line
                    && (line <= f.end
                        || (line == f.end.saturating_add(1)
                            && self.lines.get(line).is_some_and(|l| is_closing_line(l))))
            })
            .copied()
            .collect()
    }

    pub fn hidden_map(&self) -> Hidden {
        self.build_hidden(true)
    }

    /// The end state, ignoring any running animation (for cursor logic).
    pub fn hidden_final(&self) -> Hidden {
        self.build_hidden(false)
    }

    fn build_hidden(&self, animated: bool) -> Hidden {
        let anim = if animated {
            self.fold_anim
                .filter(|a| a.began.elapsed().as_millis() < FOLD_ANIM_MS)
        } else {
            None
        };
        if self.closed.is_empty() && anim.is_none() {
            return Hidden::default();
        }
        let mut ranges: Vec<(usize, usize)> = self
            .closed
            .iter()
            .filter(|&&s| anim.map_or(true, |a| a.start != s))
            .filter_map(|&s| self.fold_at(s))
            .map(|f| (f.start.saturating_add(1), f.end))
            .collect();
        if let Some(a) = anim {
            let len = a.end.saturating_sub(a.start);
            let t = (a.began.elapsed().as_secs_f32() * 1000.0 / 140.0).min(1.0);
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            let progress = if a.closing { 1.0 - eased } else { eased };
            #[allow(clippy::as_conversions, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let shown = (len as f32 * progress).round() as usize;
            if shown < len {
                ranges.push((a.start.saturating_add(1).saturating_add(shown), a.end));
            }
        }
        Hidden::new(ranges)
    }

    pub fn set_animate_folds(&mut self, on: bool) {
        self.animate_folds = on;
        if !on {
            self.fold_anim = None;
        }
    }

    pub fn fold_anim_pending(&self) -> bool {
        self.fold_anim.is_some()
    }

    pub fn fold_animating(&self) -> bool {
        self.fold_anim
            .is_some_and(|a| a.began.elapsed().as_millis() < FOLD_ANIM_MS)
    }

    pub fn finish_fold_anim(&mut self) {
        self.fold_anim = None;
    }

    fn start_fold_anim(&mut self, fold: Fold, closing: bool) {
        self.fold_anim = self.animate_folds.then(|| FoldAnim {
            start: fold.start,
            end: fold.end,
            began: Instant::now(),
            closing,
        });
    }

    pub fn closed_fold_end(&self, line: usize) -> Option<usize> {
        if self.closed.contains(&line) {
            self.fold_at(line).map(|f| f.end)
        } else {
            None
        }
    }

    pub fn closed_fold_len(&self, line: usize) -> Option<usize> {
        self.closed_fold_end(line).map(|end| end.saturating_sub(line))
    }

    pub fn fold_toggle(&mut self, line: usize) -> bool {
        self.refresh_folds();
        if self.closed.remove(&line) {
            if let Some(fold) = self.fold_at(line) {
                self.start_fold_anim(fold, false);
            }
            return true;
        }
        self.fold_close(line)
    }

    pub fn fold_open(&mut self, line: usize) -> bool {
        self.refresh_folds();
        let found = self.containing(line).into_iter().find(|f| self.closed.contains(&f.start));
        match found {
            Some(fold) => {
                let removed = self.closed.remove(&fold.start);
                if removed {
                    self.start_fold_anim(fold, false);
                }
                removed
            }
            None => false,
        }
    }

    pub fn fold_close(&mut self, line: usize) -> bool {
        self.refresh_folds();
        let found = self.containing(line).into_iter().find(|f| !self.closed.contains(&f.start));
        match found {
            Some(fold) => {
                self.closed.insert(fold.start);
                self.start_fold_anim(fold, true);
                true
            }
            None => false,
        }
    }

    pub fn fold_open_all(&mut self) {
        self.fold_anim = None;
        self.closed.clear();
    }

    pub fn fold_close_all(&mut self) -> bool {
        self.fold_anim = None;
        self.refresh_folds();
        self.closed = self.folds.iter().map(|f| f.start).collect();
        !self.closed.is_empty()
    }

    pub fn fold_reveal(&mut self, line: usize) {
        if self.closed.is_empty() {
            return;
        }
        let hiding: Vec<usize> = self
            .closed
            .iter()
            .copied()
            .filter(|&s| self.fold_at(s).is_some_and(|f| s < line && line <= f.end))
            .collect();
        for start in hiding {
            self.closed.remove(&start);
        }
    }

    pub fn fold_next_start(&mut self, line: usize) -> Option<usize> {
        self.refresh_folds();
        self.folds.iter().map(|f| f.start).find(|&s| s > line)
    }

    pub fn fold_prev_end(&mut self, line: usize) -> Option<usize> {
        self.refresh_folds();
        self.folds.iter().rev().map(|f| f.end).find(|&e| e < line)
    }
}