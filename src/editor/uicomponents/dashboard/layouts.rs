use super::*;
use std::path::Path;

// ---- Cell helpers -------------------------------------------------------

impl Cell {
    fn empty() -> Self {
        Self { text: String::new(), notes: Vec::new() }
    }
    fn put(&mut self, s: &str, kind: Option<AnnotationType>) {
        let start = self.text.len();
        self.text.push_str(s);
        if let Some(kind) = kind {
            if self.text.len() > start {
                self.notes.push((kind, start, self.text.len()));
            }
        }
    }
    fn width(&self) -> usize {
        self.text.width()
    }
    fn pad_to(&mut self, w: usize) {
        let pad = w.saturating_sub(self.width());
        self.text.push_str(&" ".repeat(pad));
    }
    fn select_all(&mut self) {
        let n = self.text.len();
        self.notes.push((selection_kind(), 0, n));
    }
    fn append(&mut self, other: &Self) {
        let base = self.text.len();
        self.text.push_str(&other.text);
        for &(kind, s, e) in &other.notes {
            self.notes.push((kind, s + base, e + base));
        }
    }
}

fn hcat(parts: &[Cell], gap: usize) -> Cell {
    let mut out = Cell::empty();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.put(&" ".repeat(gap), None);
        }
        out.append(part);
    }
    out
}

fn rule(w: usize) -> Cell {
    let mut c = Cell::empty();
    c.put(&"─".repeat(w), Some(AnnotationType::Border));
    c
}

/// Thin rounded frame, `w` columns wide in total. Body cells get 1 space of padding each side.
fn framed(title: &str, body: &[Cell], w: usize) -> Vec<Cell> {
    let inner = w.saturating_sub(4);
    let mut out = Vec::with_capacity(body.len() + 2);

    let mut top = Cell::empty();
    if title.is_empty() {
        top.put(&format!("╭{}╮", "─".repeat(w.saturating_sub(2))), Some(AnnotationType::Border));
    } else {
        top.put("╭─ ", Some(AnnotationType::Border));
        top.put(title, Some(AnnotationType::Keyword));
        top.put(" ", None);
        let dashes = w.saturating_sub(5 + title.width());
        top.put(&format!("{}╮", "─".repeat(dashes)), Some(AnnotationType::Border));
    }
    out.push(top);

    for cell in body {
        let mut line = Cell::empty();
        line.put("│ ", Some(AnnotationType::Border));
        let mut content = cell.clone();
        content.pad_to(inner);
        line.append(&content);
        line.put(" │", Some(AnnotationType::Border));
        out.push(line);
    }

    let mut bottom = Cell::empty();
    bottom.put(&format!("╰{}╯", "─".repeat(w.saturating_sub(2))), Some(AnnotationType::Border));
    out.push(bottom);
    out
}

/// Share `avail` entry slots between two lists; the smaller list keeps what it needs.
fn split(avail: usize, a: usize, b: usize) -> (usize, usize) {
    if a + b <= avail {
        return (a, b);
    }
    let half = avail / 2;
    if a <= half {
        (a, avail - a)
    } else if b <= half {
        (avail - b, b)
    } else {
        (avail - half, half)
    }
}

// ---- Building blocks ----------------------------------------------------

impl Dashboard {
    fn one(pad: usize, cell: Cell) -> AnnotatedString {
        Self::compose(pad, vec![cell])
    }

    /// Label, right-aligned coral shortcut, `▌` marker plus highlight when selected.
    fn row_cell(&self, icon: &str, label: &str, key: &str, selected: bool, w: usize) -> Cell {
        let mut c = Cell::empty();
        c.put(if selected { "▌" } else { " " }, selected.then_some(AnnotationType::Function));
        let room = w.saturating_sub(1 + KEY_W + 1);
        c.put(&fit(&format!("{icon}{label}"), room), None);
        c.pad_to(w.saturating_sub(KEY_W));
        c.put(&format!("{key:>KEY_W$}"), (!key.is_empty()).then_some(AnnotationType::Number));
        c.pad_to(w);
        if selected {
            c.select_all();
        }
        c
    }

    /// One file entry: a single dense line, or name + dim path on two lines.
    fn entry_cells(&self, pinned: bool, pos: usize, path: &str, selected: bool, w: usize, dense: bool) -> Vec<Cell> {
        let p = Path::new(path);
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
        let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
        let icon = self.icon(if pinned { PIN_ICON } else { RECENT_ICON });
        let key = Self::key_label(pinned, pos);
        let marker = if selected { "▌" } else { " " };
        let marker_kind = selected.then_some(AnnotationType::Function);
        let room = w.saturating_sub(1 + KEY_W + 1);

        let mut first = Cell::empty();
        first.put(marker, marker_kind);
        let lead = fit(&format!("{icon}{name}"), room);
        let remaining = room.saturating_sub(lead.width());
        first.put(&lead, None);
        if dense && remaining > 8 {
            first.put("  ", None);
            let dir_text = Self::label_for(dir, remaining - 2);
            first.put(&fit(&dir_text, remaining - 2), Some(AnnotationType::Comment));
        }
        first.pad_to(w.saturating_sub(KEY_W));
        first.put(&format!("{key:>KEY_W$}"), (!key.is_empty()).then_some(AnnotationType::Number));
        first.pad_to(w);

        let mut out = vec![first];
        if !dense {
            let mut second = Cell::empty();
            second.put(marker, marker_kind);
            second.put(&" ".repeat(icon.width()), None);
            let room = w.saturating_sub(1 + icon.width());
            second.put(&fit(&Self::label_for(dir, room), room), Some(AnnotationType::Comment));
            second.pad_to(w);
            out.push(second);
        }
        if selected {
            for cell in &mut out {
                cell.select_all();
            }
        }
        out
    }

    /// Entries of one list (no heading). Always returns at least one line.
    fn list_body(&self, pinned: bool, visible: usize, w: usize, dense: bool, gap: bool) -> Vec<Cell> {
        let (list, filtered) = if pinned { (&self.pinned, &self.pinned_f) } else { (&self.recent, &self.recent_f) };
        if filtered.is_empty() {
            let note = if list.is_empty() { "(none)" } else { "(no matches)" };
            return vec![Cell::text(note, AnnotationType::Punctuation, w)];
        }
        let n = visible.min(filtered.len());
        let is_active = self.active_pinned == pinned;
        let start = match self.selected {
            Some(i) if is_active && i >= n => i + 1 - n,
            _ => 0,
        };
        let mut out = Vec::new();
        for (pos, &index) in filtered.iter().enumerate().skip(start).take(n) {
            let Some(path) = list.get(index) else { continue };
            let selected = is_active && self.selected == Some(pos);
            out.extend(self.entry_cells(pinned, pos, path, selected, w, dense));
            if gap && pos + 1 < start + n {
                out.push(Cell::blank(w));
            }
        }
        out
    }

    /// Width the content wants: the longest action or filename, so nothing clips.
    fn natural_w(&self) -> usize {
        let menu = self
            .menu()
            .iter()
            .map(|m| self.icon(m.icon).width() + m.label.width())
            .max()
            .unwrap_or(0)
            + KEY_W
            + 3;
        let files = self
            .pinned
            .iter()
            .chain(self.recent.iter())
            .map(|path| {
                let p = Path::new(path);
                let name = p.file_name().and_then(|n| n.to_str()).map_or(0, |n| n.width());
                let dir = p.parent().and_then(|d| d.to_str()).map_or(0, |d| Self::label_for(d, 60).width());
                (name + 4 + KEY_W + 3).max(dir + 5)
            })
            .max()
            .unwrap_or(0);
        menu.max(files)
    }

    fn effective_layout(&self, a: Size) -> DashLayout {
        match self.layout {
            DashLayout::Right if a.width >= 100 && a.height >= 18 => DashLayout::Right,
            DashLayout::Left if a.width >= 72 && a.height >= 12 => DashLayout::Left,
            _ => DashLayout::Center,
        }
    }

    pub(super) fn render_ws(&self) -> Vec<AnnotatedString> {
        let area = self.size;
        let mut rows = match self.effective_layout(area) {
            DashLayout::Left => self.layout_left(area),
            DashLayout::Right => self.layout_right(area),
            DashLayout::Center => self.layout_center(area),
        };
        rows.truncate(area.height);
        rows
    }

    // ---- Center ----------------------------------------------------------

    fn layout_center(&self, area: Size) -> Vec<AnnotatedString> {
        let boxed = self.boxed && !self.compact;
        let inner_h = area.height.saturating_sub(if boxed { 2 } else { 0 });
        let avail_w = area.width.saturating_sub(if boxed { 6 } else { 2 });
        let two = avail_w >= 2 * MIN_COL + COL_GAP;
        let want = self.natural_w().clamp(28, 48);
        let col_w = if two { want.min((avail_w - COL_GAP) / 2) } else { want.min(avail_w) };
        let cw = if two { 2 * col_w + COL_GAP } else { col_w };

        let items = self.menu();
        let per = if two { items.len().div_ceil(2) } else { items.len() };
        let tall = inner_h >= 22 && !self.compact;
        let mid = inner_h >= 18 && !self.compact;
        let has_files = self.has_entries();
        let fixed = 1 + usize::from(tall) + usize::from(mid)   // title block
            + 1 + per                                           // Actions heading + rows
            + if has_files { 2 } else { 0 }                     // rule + files heading
            + 2                                                 // closing rule + legend
            + usize::from(self.filtering);
        let free = inner_h.saturating_sub(fixed);
        let dense = free < 6;
        let visible = (free / if dense { 1 } else { 2 }).min(MAX_VISIBLE);
        let show_files = has_files && visible > 0;

        let center = |text: &str, kind: Option<AnnotationType>| -> Cell {
            let text = fit(text, cw);
            let mut c = Cell::empty();
            c.put(&" ".repeat(cw.saturating_sub(text.width()) / 2), None);
            c.put(&text, kind);
            c.pad_to(cw);
            c
        };

        let mut body: Vec<Cell> = Vec::new();
        body.push(center(&format!("◆ vMACS  {VERSION}"), Some(AnnotationType::Keyword)));
        if tall {
            body.push(center("Versatile Modal And Command System", Some(AnnotationType::Punctuation)));
        }
        if mid {
            body.push(Cell::blank(cw));
        }

        body.push(Cell::text("Actions", AnnotationType::Keyword, cw));
        for r in 0..per {
            let at = |i: usize| {
                items.get(i).map(|it| {
                    self.row_cell(self.icon(it.icon), &it.label, it.key, self.menu_sel == Some(i), col_w)
                })
            };
            let mut parts: Vec<Cell> = at(r).into_iter().collect();
            if two {
                parts.push(at(r + per).unwrap_or_else(|| Cell::blank(col_w)));
            }
            let mut row = hcat(&parts, COL_GAP);
            row.pad_to(cw);
            body.push(row);
        }

        if show_files {
            body.push(rule(cw));
            let both = two && !self.pinned.is_empty() && !self.recent.is_empty();
            let lists: Vec<bool> = if both {
                vec![true, false]
            } else if self.pinned.is_empty() {
                vec![false]
            } else if self.recent.is_empty() || self.active_pinned {
                vec![true]
            } else {
                vec![false]
            };
            let each = if lists.len() == 2 { col_w } else { cw };
            let cols: Vec<Vec<Cell>> = lists
                .iter()
                .map(|&p| {
                    let mut col = vec![Cell::text(&self.heading(p), AnnotationType::Keyword, each)];
                    col.extend(self.list_body(p, visible, each, dense, false));
                    col
                })
                .collect();
            let rows_n = cols.iter().map(Vec::len).max().unwrap_or(0);
            for r in 0..rows_n {
                let parts: Vec<Cell> = cols
                    .iter()
                    .map(|c| {
                        c.get(r).cloned().map_or_else(|| Cell::blank(each), |mut x| {
                            x.pad_to(each);
                            x
                        })
                    })
                    .collect();
                let mut row = hcat(&parts, COL_GAP);
                row.pad_to(cw);
                body.push(row);
            }
        }

        body.push(rule(cw));
        if self.filtering {
            body.push(center(&self.filter_text(), Some(AnnotationType::Keyword)));
        }
        body.push(center(self.hint(), Some(AnnotationType::Punctuation)));
        body.truncate(inner_h);

        let lines = if boxed { framed("", &body, cw + 4) } else { body };
        let w = lines.first().map_or(cw, |c| c.width());
        let pad = area.width.saturating_sub(w) / 2;
        lines.into_iter().map(|c| Self::one(pad, c)).collect()
    }

    // ---- Left ------------------------------------------------------------

    fn layout_left(&self, area: Size) -> Vec<AnnotatedString> {
        let items = self.menu();
        let menu_nat = items
            .iter()
            .map(|m| self.icon(m.icon).width() + m.label.width())
            .max()
            .unwrap_or(0)
            + KEY_W
            + 3;
        let actions_w = (menu_nat + 4).clamp(24, 32);
        let avail = area.width.saturating_sub(4);
        let files_w = avail.saturating_sub(actions_w + 1).clamp(30, 76);
        let total_w = actions_w + 1 + files_w;
        // Left of center, and it drifts inward as the terminal widens.
        let margin = (area.width.saturating_sub(total_w) / 4).max(2);
        let inner = files_w.saturating_sub(4);

        let show_pinned = !self.pinned.is_empty();
        let chrome = 4 + usize::from(self.filtering);        // title, rule, rule, legend
        let panels = usize::from(show_pinned) + 1;
        let lines = area.height.saturating_sub(chrome + 2 * panels);
        let pinned_n = if show_pinned { self.pinned_f.len().max(1) } else { 0 };
        let (np, nr) = split(lines, pinned_n, self.recent_f.len().max(1));

        let p_body = show_pinned.then(|| self.list_body(true, np.max(1), inner, true, false));
        let mut r_body = self.list_body(false, nr.max(1), inner, true, false);
        let mut a_body: Vec<Cell> = items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                self.row_cell(self.icon(it.icon), &it.label, it.key, self.menu_sel == Some(i), actions_w - 4)
            })
            .collect();

        // Equal heights so both columns end on the same row.
        let files_h = p_body.as_ref().map_or(0, |b| b.len() + 2) + r_body.len() + 2;
        let actions_h = a_body.len() + 2;
        if actions_h > files_h {
            r_body.extend((0..actions_h - files_h).map(|_| Cell::blank(inner)));
        } else {
            a_body.extend((0..files_h - actions_h).map(|_| Cell::blank(actions_w - 4)));
        }

        let mut files: Vec<Cell> = Vec::new();
        if let Some(body) = &p_body {
            files.extend(framed(&self.heading(true), body, files_w));
        }
        files.extend(framed(&self.heading(false), &r_body, files_w));
        let actions = framed("Actions", &a_body, actions_w);

        let mut rows = Vec::new();

        // Group header: title left, version right, rule beneath.
        let version = VERSION.to_string();
        let mut title = Cell::empty();
        title.put("◆ vMACS", Some(AnnotationType::Keyword));
        title.pad_to(total_w.saturating_sub(version.width()));
        title.put(&version, Some(AnnotationType::Comment));
        rows.push(Self::one(margin, title));
        rows.push(Self::one(margin, rule(total_w)));

        for r in 0..actions.len().max(files.len()) {
            let a = actions.get(r).cloned().unwrap_or_else(|| Cell::blank(actions_w));
            let f = files.get(r).cloned().unwrap_or_else(|| Cell::blank(files_w));
            rows.push(Self::one(margin, hcat(&[a, f], 1)));
        }

        // Group footer: rule, optional filter, legend, all the same width.
        rows.push(Self::one(margin, rule(total_w)));
        if self.filtering {
            let mut c = Cell::empty();
            c.put(&self.filter_text(), Some(AnnotationType::Keyword));
            rows.push(Self::one(margin, c));
        }
        let mut hint = Cell::empty();
        hint.put(&fit(self.hint(), total_w), Some(AnnotationType::Punctuation));
        rows.push(Self::one(margin, hint));
        rows
    }

    // ---- Right -----------------------------------------------------------

    fn layout_right(&self, area: Size) -> Vec<AnnotatedString> {
        let items = self.menu();
        let panel_w = (self.natural_w() + 6).clamp(46, 60).min(area.width / 2);
        let brand_w = 34usize;
        let gap = 6usize;
        let group_w = brand_w + gap + panel_w;
        // The brand + panel pair is one group, placed right of center.
        let start = (area.width.saturating_sub(group_w) * 2 / 3).max(2);
        let x = start + brand_w + gap;
        let inner = panel_w.saturating_sub(4);

        let has_p = !self.pinned.is_empty();
        let has_r = !self.recent.is_empty();
        let n_sec = usize::from(has_p) + usize::from(has_r);
        let h = area.height.saturating_sub(2);
        let fixed = 1 + items.len() + 1 + n_sec * 2 + 1 + usize::from(self.filtering);
        let free = h.saturating_sub(fixed);

        let np_total = if has_p { self.pinned_f.len().clamp(1, MAX_VISIBLE) } else { 0 };
        let nr_total = if has_r { self.recent_f.len().clamp(1, MAX_VISIBLE) } else { 0 };
        let want = np_total + nr_total;
        let (dense, gap_rows, per) = if free >= want * 3 {
            (false, true, 3)
        } else if free >= want * 2 {
            (false, false, 2)
        } else {
            (true, false, 1)
        };
        let (np, nr) = split(free / per, np_total, nr_total);

        let mut body = vec![Cell::text("Actions", AnnotationType::Keyword, inner)];
        for (i, it) in items.iter().enumerate() {
            body.push(self.row_cell(self.icon(it.icon), &it.label, it.key, self.menu_sel == Some(i), inner));
        }
        body.push(Cell::blank(inner));
        for (pinned, present, n) in [(true, has_p, np), (false, has_r, nr)] {
            if !present {
                continue;
            }
            body.push(Cell::text(&self.heading(pinned), AnnotationType::Keyword, inner));
            body.extend(self.list_body(pinned, n.max(1), inner, dense, gap_rows));
            body.push(Cell::blank(inner));
        }
        if self.filtering {
            body.push(Cell::text(&self.filter_text(), AnnotationType::Keyword, inner));
        }
        body.push(Cell::text(&fit(self.hint(), inner), AnnotationType::Punctuation, inner));
        body.truncate(h);

        let frame = framed("Workspace", &body, panel_w);

        // Branding: vertically centered on the panel, next to it rather than at the screen edge.
        let base = (frame.len() / 2).saturating_sub(2);
        let version = VERSION.to_string();
        frame
            .iter()
            .enumerate()
            .map(|(r, line)| {
                let mut c = Cell::empty();
                c.put(&" ".repeat(start), None);
                match r.checked_sub(base) {
                    Some(0) => c.put(&fit("◆ vMACS", brand_w), Some(AnnotationType::Keyword)),
                    Some(1) => c.put(&"─".repeat(12), Some(AnnotationType::Border)),
                    Some(2) => c.put(
                        &fit("Versatile Modal And Command System", brand_w),
                        Some(AnnotationType::Comment),
                    ),
                    Some(3) => c.put(&version, Some(AnnotationType::Comment)),
                    _ => {}
                }
                c.pad_to(x);
                c.append(line);
                Self::one(0, c)
            })
            .collect()
    }
}