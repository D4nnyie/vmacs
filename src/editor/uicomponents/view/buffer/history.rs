use super::{Buffer, Line};
use crate::prelude::*;

const MAX_UNDO_GROUPS: usize = 1000;

enum Op {
    SetLine { idx: LineIdx, old: String, new: String },
    InsertLine { idx: LineIdx, content: String },
    RemoveLine { idx: LineIdx, content: String },
}

struct UndoGroup {
    ops: Vec<Op>,
    cursor_before: Location,
    cursor_after: Location,
}

#[derive(Default)]
pub struct History {
    open: Vec<Op>,
    open_cursor: Option<Location>,
    undo: Vec<UndoGroup>,
    redo: Vec<UndoGroup>,
    saved_depth: Option<usize>,
}

impl Buffer {
    pub(super) fn op_set_line(&mut self, idx: LineIdx, new: Line) {
        let Some(slot) = self.lines.get_mut(idx) else {
            return;
        };
        let old = slot.to_string();
        let new_text = new.to_string();
        if old == new_text {
            return;
        }
        *slot = new;
        self.push_set_line(idx, old, new_text);
        self.touch();
    }

    pub(super) fn edit_line<R>(
        &mut self,
        idx: LineIdx,
        edit: impl FnOnce(&mut Line) -> R,
    ) -> Option<R> {
        let line = self.lines.get_mut(idx)?;
        let old = line.to_string();
        let result = edit(line);
        let new = line.to_string();
        if old != new {
            self.push_set_line(idx, old, new);
            self.touch();
        }
        Some(result)
    }

    pub(super) fn op_insert_line(&mut self, idx: LineIdx, line: Line) {
        let idx = idx.min(self.lines.len());
        self.history.open.push(Op::InsertLine {
            idx,
            content: line.to_string(),
        });
        self.lines.insert(idx, line);
        self.shift_closed(idx, true);
        self.touch();
    }

    pub(super) fn op_remove_line(&mut self, idx: LineIdx) -> Option<Line> {
        if idx >= self.lines.len() {
            return None;
        }
        let line = self.lines.remove(idx);
        self.shift_closed(idx, false);
        self.history.open.push(Op::RemoveLine {
            idx,
            content: line.to_string(),
        });
        self.touch();
        Some(line)
    }

    fn push_set_line(&mut self, idx: LineIdx, old: String, new: String) {
        if let Some(Op::SetLine {
            idx: last_idx,
            new: last_new,
            ..
        }) = self.history.open.last_mut()
        {
            if *last_idx == idx {
                *last_new = new;
                return;
            }
        }
        self.history.open.push(Op::SetLine { idx, old, new });
    }

    pub fn mark_cursor(&mut self, at: Location) {
        if self.history.open.is_empty() {
            self.history.open_cursor = Some(at);
        }
    }

    pub fn seal_undo_group(&mut self, cursor_after: Location) {
        let history = &mut self.history;
        if history.open.is_empty() {
            history.open_cursor = None;
            return;
        }
        let ops = std::mem::take(&mut history.open);
        let cursor_before = history.open_cursor.take().unwrap_or(cursor_after);

        if !history.redo.is_empty() {
            if history
                .saved_depth
                .is_some_and(|depth| depth > history.undo.len())
            {
                history.saved_depth = None;
            }
            history.redo.clear();
        }

        history.undo.push(UndoGroup {
            ops,
            cursor_before,
            cursor_after,
        });

        if history.undo.len() > MAX_UNDO_GROUPS {
            history.undo.remove(0);
            history.saved_depth = match history.saved_depth {
                Some(depth) if depth > 0 => Some(depth.saturating_sub(1)),
                _ => None,
            };
        }
    }

    pub(super) fn mark_saved(&mut self) {
        self.history.saved_depth = Some(self.history.undo.len());
    }

    pub fn undo(&mut self, current_cursor: Location) -> Option<Location> {
        self.seal_undo_group(current_cursor);
        let group = self.history.undo.pop()?;
        for op in group.ops.iter().rev() {
            self.apply_op(op, false);
        }
        let cursor = group.cursor_before;
        self.history.redo.push(group);
        self.after_history_move();
        Some(cursor)
    }

    pub fn redo(&mut self, current_cursor: Location) -> Option<Location> {
        self.seal_undo_group(current_cursor);
        let group = self.history.redo.pop()?;
        for op in &group.ops {
            self.apply_op(op, true);
        }
        let cursor = group.cursor_after;
        self.history.undo.push(group);
        self.after_history_move();
        Some(cursor)
    }

    fn apply_op(&mut self, op: &Op, forward: bool) {
        match op {
            Op::SetLine { idx, old, new } => {
                let text = if forward { new } else { old };
                if let Some(line) = self.lines.get_mut(*idx) {
                    *line = Line::from(text);
                }
            }
            Op::InsertLine { idx, content } => {
                if forward {
                    self.insert_raw(*idx, content);
                } else {
                    self.remove_raw(*idx);
                }
            }
            Op::RemoveLine { idx, content } => {
                if forward {
                    self.remove_raw(*idx);
                } else {
                    self.insert_raw(*idx, content);
                }
            }
        }
    }

    fn insert_raw(&mut self, idx: LineIdx, content: &str) {
        let idx = idx.min(self.lines.len());
        self.lines.insert(idx, Line::from(content));
        self.shift_closed(idx, true);
    }

    fn remove_raw(&mut self, idx: LineIdx) {
        if idx < self.lines.len() {
            self.lines.remove(idx);
            self.shift_closed(idx, false);
        }
    }

    fn after_history_move(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        let clean = self
            .history
            .saved_depth
            .map_or(self.history.undo.is_empty(), |depth| {
                depth == self.history.undo.len()
            });
        self.dirty = !clean;
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn loc(line_idx: usize, grapheme_idx: usize) -> Location {
        Location { line_idx, grapheme_idx }
    }

    fn open(dir: &TempDir, text: &str) -> Buffer {
        let path = dir.write("f.txt", text.as_bytes());
        Buffer::load(path.to_str().unwrap()).unwrap()
    }

    #[test]
    fn undo_and_redo_restore_text_and_cursor() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "abc\n");
        buffer.mark_cursor(loc(0, 1));
        buffer.insert_char('x', loc(0, 1));
        assert_eq!(buffer.line_content(0).as_deref(), Some("axbc"));

        let cursor = buffer.undo(loc(0, 2)).unwrap();
        assert_eq!(buffer.line_content(0).as_deref(), Some("abc"));
        assert_eq!(cursor.grapheme_idx, 1);

        let cursor = buffer.redo(loc(0, 1)).unwrap();
        assert_eq!(buffer.line_content(0).as_deref(), Some("axbc"));
        assert_eq!(cursor.grapheme_idx, 2);
    }

    #[test]
    fn edits_without_a_seal_are_one_step() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "abc\n");
        buffer.insert_char('x', loc(0, 1));
        buffer.insert_char('y', loc(0, 2));
        buffer.undo(loc(0, 3));
        assert_eq!(buffer.line_content(0).as_deref(), Some("abc"));
        assert!(buffer.undo(loc(0, 0)).is_none());
    }

    #[test]
    fn line_inserts_and_removals_undo() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "abc\n");
        buffer.insert_line(1, "new");
        assert_eq!(buffer.height(), 2);
        buffer.undo(loc(1, 0));
        assert_eq!(buffer.height(), 1);

        buffer.delete_line(0);
        assert_eq!(buffer.height(), 0);
        buffer.undo(loc(0, 0));
        assert_eq!(buffer.line_content(0).as_deref(), Some("abc"));
    }

    #[test]
    fn new_edit_clears_redo() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "abc\n");
        buffer.insert_char('x', loc(0, 0));
        buffer.undo(loc(0, 1));
        buffer.insert_char('z', loc(0, 0));
        assert!(buffer.redo(loc(0, 1)).is_none());
        assert_eq!(buffer.line_content(0).as_deref(), Some("zabc"));
    }

    #[test]
    fn dirty_flag_tracks_the_saved_state() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "abc\n");
        buffer.insert_char('x', loc(0, 0));
        buffer.seal_undo_group(loc(0, 1));
        buffer.save().unwrap();
        assert!(!buffer.is_dirty());

        buffer.insert_char('y', loc(0, 0));
        buffer.seal_undo_group(loc(0, 1));
        assert!(buffer.is_dirty());

        buffer.undo(loc(0, 1));
        assert!(!buffer.is_dirty(), "back at the saved state");
        buffer.undo(loc(0, 1));
        assert!(buffer.is_dirty(), "before the saved state");
        buffer.redo(loc(0, 0));
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn history_is_capped() {
        let dir = TempDir::new();
        let mut buffer = open(&dir, "\n");
        for _ in 0..(MAX_UNDO_GROUPS + 5) {
            buffer.mark_cursor(loc(0, 0));
            buffer.insert_char('a', loc(0, 0));
            buffer.seal_undo_group(loc(0, 1));
        }
        let mut steps = 0;
        while buffer.undo(loc(0, 0)).is_some() {
            steps += 1;
        }
        assert_eq!(steps, MAX_UNDO_GROUPS);
    }
}
