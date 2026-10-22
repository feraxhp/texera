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

//! Viewport scrolling and cursor tracking logic.

use std::ops::Range;
use crate::viewport::metrics::CellMetrics;
use crate::viewport::visual_line::VisualPosition;

/// Viewport scrolling controller managing visible bounds and cursor tracking.
///
/// # Examples
///
/// ```rust
/// use texera::{Scroller, VisualPosition};
///
/// let mut scroller = Scroller::new(24, 80);
/// scroller.ensure_visible(VisualPosition { visual_row: 30, visual_col: 10 }, false);
/// assert!(scroller.scroll_y > 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scroller {
    /// Horizontal scroll offset (in visual terminal columns).
    pub scroll_x: usize,
    /// Vertical scroll offset (in visual lines).
    pub scroll_y: usize,
    /// Number of lines currently visible inside the editor area.
    pub visible_lines: usize,
    /// Number of terminal columns currently visible inside the editor area.
    pub visible_columns: usize,
    /// Margin (in cells) to keep between the cursor and the horizontal edge of the viewport.
    pub margin_horizontal: usize,
    /// Margin (in lines) to keep between the cursor and the vertical edge of the viewport.
    pub margin_vertical: usize,
}

impl Default for Scroller {
    fn default() -> Self {
        Self {
            scroll_x: 0,
            scroll_y: 0,
            visible_lines: 24,
            visible_columns: 80,
            margin_horizontal: 2,
            margin_vertical: 1,
        }
    }
}

impl Scroller {
    /// Creates a new [`Scroller`] with the given viewport height and width.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::Scroller;
    ///
    /// let scroller = Scroller::new(20, 80);
    /// assert_eq!(scroller.visible_lines, 20);
    /// assert_eq!(scroller.visible_columns, 80);
    /// ```
    pub fn new(visible_lines: usize, visible_columns: usize) -> Self {
        Self {
            visible_lines: visible_lines.max(1),
            visible_columns: visible_columns.max(1),
            ..Default::default()
        }
    }

    /// Update viewport dimensions, respecting optional `max_lines` constraint.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::Scroller;
    ///
    /// let mut scroller = Scroller::default();
    /// scroller.update_dimensions(30, 100, Some(25));
    /// assert_eq!(scroller.visible_lines, 25);
    /// assert_eq!(scroller.visible_columns, 100);
    /// ```
    pub fn update_dimensions(&mut self, height: usize, width: usize, max_lines: Option<usize>) {
        let eff_height = if let Some(max) = max_lines {
            height.min(max)
        } else {
            height
        };
        self.visible_lines = eff_height.max(1);
        self.visible_columns = width.max(1);
    }

    /// Autoscroll to ensure the given visual cursor position is comfortably visible within margins.
    ///
    /// When soft wrapping is enabled, `scroll_x` is locked to 0.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{Scroller, VisualPosition};
    ///
    /// let mut scroller = Scroller::new(10, 40);
    /// scroller.ensure_visible(VisualPosition { visual_row: 15, visual_col: 5 }, false);
    /// assert_eq!(scroller.scroll_y, 7);
    /// ```
    pub fn ensure_visible(&mut self, cursor: VisualPosition, wrap: bool) {
        if wrap {
            self.scroll_x = 0;
        } else {
            // Horizontal autoscroll
            let right_margin = self.margin_horizontal.min(self.visible_columns.saturating_sub(1));
            let left_bound = self.scroll_x;
            let right_bound = self.scroll_x + self.visible_columns.saturating_sub(right_margin + 1);

            if cursor.visual_col < left_bound + self.margin_horizontal {
                self.scroll_x = cursor.visual_col.saturating_sub(self.margin_horizontal);
            } else if cursor.visual_col > right_bound {
                self.scroll_x = (cursor.visual_col + right_margin + 1)
                    .saturating_sub(self.visible_columns);
            }
        }

        // Vertical autoscroll
        let bottom_margin = self.margin_vertical.min(self.visible_lines.saturating_sub(1));
        let top_bound = self.scroll_y;
        let bottom_bound = self.scroll_y + self.visible_lines.saturating_sub(bottom_margin + 1);

        if cursor.visual_row < top_bound + self.margin_vertical {
            self.scroll_y = cursor.visual_row.saturating_sub(self.margin_vertical);
        } else if cursor.visual_row > bottom_bound {
            self.scroll_y = (cursor.visual_row + bottom_margin + 1)
                .saturating_sub(self.visible_lines);
        }
    }

    /// Returns the 0-based range of visual rows currently in view.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::Scroller;
    ///
    /// let mut scroller = Scroller::new(10, 80);
    /// scroller.scroll_y = 5;
    /// assert_eq!(scroller.visible_rows_range(20), 5..15);
    /// ```
    pub fn visible_rows_range(&self, total_visual_lines: usize) -> Range<usize> {
        if total_visual_lines == 0 {
            return 0..0;
        }
        let start = self.scroll_y.min(total_visual_lines);
        let end = (start + self.visible_lines).min(total_visual_lines);
        start..end
    }

    /// Horizontally crop a line string to the visible window `[scroll_x, scroll_x + visible_columns]`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::Scroller;
    ///
    /// let mut scroller = Scroller::new(10, 5);
    /// scroller.scroll_x = 2;
    /// let cropped = scroller.slice_horizontal("abcdefgh", 4);
    /// assert_eq!(cropped, "cdefg");
    /// ```
    pub fn slice_horizontal(&self, text: &str, tab_size: usize) -> String {
        if self.scroll_x == 0 && CellMetrics::string_width(text, 0, tab_size) <= self.visible_columns {
            return text.to_string();
        }

        let mut result = String::new();
        let mut current_col = 0;
        let start_col = self.scroll_x;
        let end_col = self.scroll_x + self.visible_columns;

        for ch in text.chars() {
            let width = CellMetrics::char_width(ch, current_col, tab_size);
            let next_col = current_col + width;

            if next_col > start_col && current_col < end_col {
                if ch == '\t' {
                    let spaces = width;
                    for sc in 0..spaces {
                        let c = current_col + sc;
                        if c >= start_col && c < end_col {
                            result.push(' ');
                        }
                    }
                } else {
                    result.push(ch);
                }
            }

            current_col = next_col;
            if current_col >= end_col {
                break;
            }
        }

        result
    }
}
