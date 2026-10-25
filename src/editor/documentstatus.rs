use crate::prelude::*;

#[derive(Default, Eq, PartialEq, Debug)]
pub struct DocumentStatus {
    pub total_lines: usize,
    pub current_line_idx: LineIdx,
    pub current_column: GraphemeIdx,
    pub is_modified: bool,
    pub file_name: String,
    pub file_type: String,
    pub encoding: &'static str,
    pub line_ending: &'static str,
    pub indent: &'static str,
    pub file_path: String,
    pub read_only: bool,
}

impl DocumentStatus {
    pub fn line_count_to_string(&self) -> String {
        format!("{} lines", self.total_lines)
    }
    pub fn position_indicator_to_string(&self) -> String {
        format!(
            "Ln {}/{}, Col {}",
            self.current_line_idx.saturating_add(1),
            self.total_lines,
            self.current_column.saturating_add(1)
        )
    }
    pub fn position_compact(&self) -> String {
        format!(
            "{}:{}",
            self.current_line_idx.saturating_add(1),
            self.current_column.saturating_add(1)
        )
    }
}