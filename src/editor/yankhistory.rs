use std::collections::VecDeque;

pub const MAX_ENTRIES_LIMIT: usize = 100;
pub const MAX_ENTRY_BYTES_LIMIT: usize = 64 * 1024;

#[derive(Default)]
pub struct YankHistory {
    enabled: bool,
    max_entries: usize,
    max_entry_bytes: usize,
    entries: VecDeque<String>,
}

impl YankHistory {
    pub fn configure(&mut self, enabled: bool, max_entries: usize, max_entry_bytes: usize) {
        self.enabled = enabled;
        self.max_entries = max_entries.min(MAX_ENTRIES_LIMIT);
        self.max_entry_bytes = max_entry_bytes.min(MAX_ENTRY_BYTES_LIMIT);
        if !self.is_enabled() {
            self.entries.clear();
            return;
        }
        let limit = self.max_entry_bytes;
        self.entries.retain(|entry| entry.len() <= limit);
        self.entries.truncate(self.max_entries);
    }

    pub const fn is_enabled(&self) -> bool {
        self.enabled && self.max_entries > 0 && self.max_entry_bytes > 0
    }

    pub fn record(&mut self, text: &str) {
        if !self.is_enabled() || text.trim().is_empty() || text.len() > self.max_entry_bytes {
            return;
        }
        self.entries.retain(|entry| entry != text);
        self.entries.push_front(text.to_string());
        self.entries.truncate(self.max_entries);
    }

    pub fn get(&self, idx: usize) -> Option<&str> {
        self.entries.get(idx).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    pub fn clear(&mut self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(max_entries: usize, max_bytes: usize) -> YankHistory {
        let mut h = YankHistory::default();
        h.configure(true, max_entries, max_bytes);
        h
    }

    #[test]
    fn off_by_default_records_nothing() {
        let mut h = YankHistory::default();
        h.record("secret");
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn newest_first_and_duplicates_move_to_front() {
        let mut h = on(5, 100);
        h.record("a");
        h.record("b");
        h.record("a");
        assert_eq!(h.entries().collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn count_is_bounded() {
        let mut h = on(2, 100);
        for text in ["a", "b", "c"] {
            h.record(text);
        }
        assert_eq!(h.entries().collect::<Vec<_>>(), vec!["c", "b"]);
    }

    #[test]
    fn oversized_and_blank_copies_are_skipped() {
        let mut h = on(5, 3);
        h.record("abcd");
        h.record("  \n ");
        assert_eq!(h.len(), 0);
        h.record("abc");
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn hard_limits_apply_to_config_values() {
        let mut h = YankHistory::default();
        h.configure(true, usize::MAX, usize::MAX);
        assert_eq!(h.max_entries, MAX_ENTRIES_LIMIT);
        assert_eq!(h.max_entry_bytes, MAX_ENTRY_BYTES_LIMIT);
    }

    #[test]
    fn reconfiguring_trims_or_wipes() {
        let mut h = on(5, 100);
        for text in ["a", "bb", "c"] {
            h.record(text);
        }
        h.configure(true, 2, 1);
        assert_eq!(h.entries().collect::<Vec<_>>(), vec!["c", "a"]);
        h.configure(false, 2, 1);
        assert_eq!(h.len(), 0);
    }
}