// Copyright (C) 2026 feraxhp
// 
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// 
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
// 
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Unsaved lines tracking plugin powered by Myers diff.

use std::collections::HashSet;

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Span;
use texera_core::State;

use crate::traits::GutterMarker;
use crate::traits::ViewPlugin;
use crate::traits::ViewUpdate;

/// Plugin that tracks unsaved/modified lines and renders an indicator beside the gutter.
///
/// Uses the Myers diff algorithm to compare the current document against the last saved snapshot,
/// marking changed or newly inserted lines.
///
/// # Examples
///
/// ```rust
/// use texera::UnsavedLinePlugin;
/// use texera_core::State;
///
/// let mut plugin = UnsavedLinePlugin::new();
/// let state = State::from("original content");
/// plugin.mark_as_saved(&state);
/// assert!(!plugin.is_dirty());
/// ```
#[derive(Clone, Debug)]
pub struct UnsavedLinePlugin {
    /// Set of 1-based line numbers that are currently dirty (modified or added).
    pub dirty_lines: HashSet<usize>,
    /// Visual character/symbol rendered in the gutter for dirty lines (default: `"┃"`).
    pub indicator_symbol: &'static str,
    /// Style applied to the dirty indicator (default: `Color::Yellow`).
    pub indicator_style: Style,
    /// Trimmed text lines from the last cleanly saved snapshot baseline.
    pub saved_snapshot: Option<Vec<String>>,
    /// Indicates whether lines were deleted compared to the saved snapshot.
    pub has_deletions: bool,
}

impl Default for UnsavedLinePlugin {
    fn default() -> Self {
        Self {
            dirty_lines: HashSet::new(),
            indicator_symbol: "┃",
            indicator_style: Style::default().fg(Color::Yellow),
            saved_snapshot: None,
            has_deletions: false,
        }
    }
}

impl UnsavedLinePlugin {
    /// Creates a new default [`UnsavedLinePlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let plugin = UnsavedLinePlugin::new();
    /// assert_eq!(plugin.indicator_symbol, "┃");
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Customize the indicator symbol (default: `"┃"`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let plugin = UnsavedLinePlugin::new().with_symbol("▎");
    /// assert_eq!(plugin.indicator_symbol, "▎");
    /// ```
    pub fn with_symbol(mut self, symbol: &'static str) -> Self {
        self.indicator_symbol = symbol;
        self
    }

    /// Customize the indicator style (default: `Color::Yellow`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::UnsavedLinePlugin;
    ///
    /// let plugin = UnsavedLinePlugin::new()
    ///     .with_style(Style::default().fg(Color::Green));
    /// ```
    pub fn with_style(mut self, style: Style) -> Self {
        self.indicator_style = style;
        self
    }

    /// Mark the given document state as cleanly saved, setting it as the new baseline snapshot.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    /// use texera_core::State;
    ///
    /// let mut plugin = UnsavedLinePlugin::new();
    /// let state = State::from("clean code");
    /// plugin.mark_as_saved(&state);
    /// assert!(!plugin.is_dirty());
    /// ```
    pub fn mark_as_saved(&mut self, state: &State) {
        let mut lines = Vec::new();
        for i in 0..state.doc().len_lines() {
            if let Ok(line) = state.doc().line(i) {
                lines.push(line.text_trimmed().to_string());
            }
        }
        self.saved_snapshot = Some(lines);
        self.dirty_lines.clear();
        self.has_deletions = false;
    }

    /// Clear all dirty lines manually.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let mut plugin = UnsavedLinePlugin::new();
    /// plugin.mark_line_dirty(1);
    /// assert!(plugin.is_dirty());
    /// plugin.reset_dirty();
    /// assert!(!plugin.is_dirty());
    /// ```
    pub fn reset_dirty(&mut self) {
        self.dirty_lines.clear();
        self.has_deletions = false;
    }

    /// Inspect current document state against the saved snapshot to detect modified lines.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    /// use texera_core::State;
    ///
    /// let mut plugin = UnsavedLinePlugin::new();
    /// let state = State::from("line 1\nline 2");
    /// plugin.mark_as_saved(&state);
    ///
    /// let modified_state = State::from("line 1\nline 2 modified");
    /// plugin.check_dirty_lines(&modified_state);
    /// assert!(plugin.is_line_dirty(2));
    /// ```
    pub fn check_dirty_lines(&mut self, state: &State) {
        let snapshot = match self.saved_snapshot.as_ref() {
            Some(s) => s,
            None => {
                // If no snapshot exists yet, initialize baseline with current state
                self.mark_as_saved(state);
                return;
            }
        };

        self.dirty_lines.clear();
        self.has_deletions = false;
        let num_lines = state.doc().len_lines();

        // 1. Find common prefix (no string allocations)
        let mut prefix = 0;
        while prefix < snapshot.len() && prefix < num_lines {
            if let Ok(line) = state.doc().line(prefix) {
                if line.text_trimmed() == snapshot[prefix] {
                    prefix += 1;
                    continue;
                }
            }
            break;
        }

        // 2. Find common suffix (no string allocations)
        let mut suffix = 0;
        while suffix < (snapshot.len() - prefix) && suffix < (num_lines - prefix) {
            let curr_idx = num_lines - 1 - suffix;
            let snap_idx = snapshot.len() - 1 - suffix;
            if let Ok(line) = state.doc().line(curr_idx) {
                if line.text_trimmed() == snapshot[snap_idx] {
                    suffix += 1;
                    continue;
                }
            }
            break;
        }

        let n = snapshot.len() - prefix - suffix;
        let m = num_lines - prefix - suffix;

        if n == 0 && m == 0 {
            return;
        }

        if n == 0 && m > 0 {
            for j in 0..m {
                self.dirty_lines.insert(prefix + j + 1);
            }
            return;
        }

        if n > 0 && m == 0 {
            self.has_deletions = true;
            return;
        }

        // Extract modified middle slices
        let a_mid = &snapshot[prefix..prefix + n];
        let mut b_mid = Vec::with_capacity(m);
        for i in 0..m {
            if let Ok(line) = state.doc().line(prefix + i) {
                b_mid.push(line.text_trimmed().to_string());
            } else {
                b_mid.push(String::new());
            }
        }

        let had_deletions = diff_slices(a_mid, &b_mid, prefix, &mut self.dirty_lines);
        if had_deletions {
            self.has_deletions = true;
        }
    }

    /// Check if the document has any unsaved modifications.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let plugin = UnsavedLinePlugin::new();
    /// assert!(!plugin.is_dirty());
    /// ```
    pub fn is_dirty(&self) -> bool {
        !self.dirty_lines.is_empty() || self.has_deletions
    }

    /// Check if a specific 1-based line is unsaved/dirty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let mut plugin = UnsavedLinePlugin::new();
    /// plugin.mark_line_dirty(5);
    /// assert!(plugin.is_line_dirty(5));
    /// assert!(!plugin.is_line_dirty(1));
    /// ```
    pub fn is_line_dirty(&self, line_number: usize) -> bool {
        self.dirty_lines.contains(&line_number)
    }

    /// Manually mark a 1-based line index as dirty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::UnsavedLinePlugin;
    ///
    /// let mut plugin = UnsavedLinePlugin::new();
    /// plugin.mark_line_dirty(3);
    /// assert!(plugin.is_line_dirty(3));
    /// ```
    pub fn mark_line_dirty(&mut self, line_number: usize) {
        self.dirty_lines.insert(line_number);
    }
}

/// Myers diff algorithm to align modified middle slices and mark only new/modified lines as dirty.
fn diff_slices(
    a_mid: &[String],
    b_mid: &[String],
    prefix: usize,
    dirty_lines: &mut HashSet<usize>,
) -> bool {
    let n = a_mid.len();
    let m = b_mid.len();

    if n == 0 && m == 0 {
        return false;
    }
    if n == 0 {
        for j in 0..m {
            dirty_lines.insert(prefix + j + 1);
        }
        return false;
    }
    if m == 0 {
        return true;
    }

    const MAX_MYERS_D: usize = 500;
    let max_d = (n + m).min(MAX_MYERS_D);
    let offset = (n + m + 1) as isize;
    let mut v = vec![0isize; 2 * (n + m) + 3];
    v[(1 + offset) as usize] = 0;

    let mut trace = Vec::with_capacity(max_d + 1);
    let mut found_d = None;

    for d in 0..=max_d {
        let d_isize = d as isize;
        let mut reached = false;

        for k in (-d_isize..=d_isize).step_by(2) {
            let mut x = if k == -d_isize
                || (k != d_isize
                    && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize])
            {
                v[(k + 1 + offset) as usize]
            } else {
                v[(k - 1 + offset) as usize] + 1
            };
            let mut y = x - k;

            while (x as usize) < n && (y as usize) < m && a_mid[x as usize] == b_mid[y as usize] {
                x += 1;
                y += 1;
            }

            v[(k + offset) as usize] = x;

            if (x as usize) >= n && (y as usize) >= m {
                reached = true;
            }
        }

        trace.push(v.clone());

        if reached {
            found_d = Some(d);
            break;
        }
    }

    let d = match found_d {
        Some(d) => d,
        None => {
            // Edit distance exceeds MAX_MYERS_D; fall back to marking all as dirty.
            for j in 0..m {
                dirty_lines.insert(prefix + j + 1);
            }
            return true;
        }
    };

    let mut x = n as isize;
    let mut y = m as isize;
    let mut a_clean = vec![false; n];
    let mut b_clean = vec![false; m];

    for step in (1..=d).rev() {
        let k = x - y;
        let step_isize = step as isize;
        let v_prev = &trace[step - 1];

        let prev_k = if k == -step_isize
            || (k != step_isize
                && v_prev[(k - 1 + offset) as usize] < v_prev[(k + 1 + offset) as usize])
        {
            k + 1
        } else {
            k - 1
        };

        let prev_x = v_prev[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;

        let (diag_x, diag_y) = if prev_k == k + 1 {
            (prev_x, prev_y + 1)
        } else {
            (prev_x + 1, prev_y)
        };

        let mut cur_x = diag_x;
        let mut cur_y = diag_y;
        while cur_x < x && cur_y < y {
            if (cur_x as usize) < n && (cur_y as usize) < m {
                a_clean[cur_x as usize] = true;
                b_clean[cur_y as usize] = true;
            }
            cur_x += 1;
            cur_y += 1;
        }

        x = prev_x;
        y = prev_y;
    }

    let mut cur_x = 0;
    let mut cur_y = 0;
    while cur_x < x && cur_y < y {
        if (cur_x as usize) < n && (cur_y as usize) < m {
            a_clean[cur_x as usize] = true;
            b_clean[cur_y as usize] = true;
        }
        cur_x += 1;
        cur_y += 1;
    }

    for j in 0..m {
        if !b_clean[j] {
            dirty_lines.insert(prefix + j + 1);
        }
    }

    a_clean.iter().any(|&clean| !clean)
}

impl ViewPlugin for UnsavedLinePlugin {
    fn update(&mut self, update: &ViewUpdate) {
        if update.doc_changed {
            self.check_dirty_lines(update.state);
        }
    }
}

impl GutterMarker for UnsavedLinePlugin {
    fn render_marker(&self, line_number: usize, _is_wrapped: bool) -> Option<Span<'static>> {
        if self.is_line_dirty(line_number) {
            Some(Span::styled(self.indicator_symbol, self.indicator_style))
        } else {
            Some(Span::raw(" "))
        }
    }
}
