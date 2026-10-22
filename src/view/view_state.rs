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
// along with this program. If not, see <https://www.gnu.org/licenses/>.\n\n//! Stateful editor view context preserved across render frames.

use ratatui::layout::Rect;
use crate::viewport::scroller::Scroller;
use crate::viewport::visual_line::VisualPosition;

/// Mutable state of the editor view persisted across render frames.
///
/// Keeps track of scrolling offsets, cached viewport layout, and syntax parsing readiness.
///
/// # Examples
///
/// ```rust
/// use texera::EditorViewState;
///
/// let mut view_state = EditorViewState::new();
/// view_state.set_scroll(0, 10);
/// assert_eq!(view_state.scroll_y(), 10);
/// ```
#[derive(Clone, Debug, Default)]
pub struct EditorViewState {
    /// Internal scroller engine managing offsets and viewport metrics.
    pub scroller: Scroller,
    /// Last rendered area geometry.
    pub last_area: Option<Rect>,
}

impl EditorViewState {
    /// Creates a new default [`EditorViewState`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let view_state = EditorViewState::new();
    /// assert_eq!(view_state.scroll_x(), 0);
    /// assert_eq!(view_state.scroll_y(), 0);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Horizontal scroll offset (in visual terminal cells/columns).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let view_state = EditorViewState::new();
    /// assert_eq!(view_state.scroll_x(), 0);
    /// ```
    pub fn scroll_x(&self) -> usize {
        self.scroller.scroll_x
    }

    /// Vertical scroll offset (in visual lines).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let view_state = EditorViewState::new();
    /// assert_eq!(view_state.scroll_y(), 0);
    /// ```
    pub fn scroll_y(&self) -> usize {
        self.scroller.scroll_y
    }

    /// Set scroll offsets manually.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let mut view_state = EditorViewState::new();
    /// view_state.set_scroll(5, 12);
    /// assert_eq!(view_state.scroll_x(), 5);
    /// assert_eq!(view_state.scroll_y(), 12);
    /// ```
    pub fn set_scroll(&mut self, scroll_x: usize, scroll_y: usize) {
        self.scroller.scroll_x = scroll_x;
        self.scroller.scroll_y = scroll_y;
    }

    /// Reset scrolling offsets to top-left (0, 0).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let mut view_state = EditorViewState::new();
    /// view_state.set_scroll(10, 20);
    /// view_state.reset_scroll();
    /// assert_eq!(view_state.scroll_x(), 0);
    /// assert_eq!(view_state.scroll_y(), 0);
    /// ```
    pub fn reset_scroll(&mut self) {
        self.scroller.scroll_x = 0;
        self.scroller.scroll_y = 0;
    }

    /// Keep cursor position visible inside the viewport bounds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{EditorViewState, VisualPosition};
    ///
    /// let mut view_state = EditorViewState::new();
    /// view_state.ensure_cursor_visible(VisualPosition { visual_row: 25, visual_col: 5 }, false);
    /// assert!(view_state.scroll_y() > 0);
    /// ```
    pub fn ensure_cursor_visible(&mut self, cursor: VisualPosition, wrap: bool) {
        self.scroller.ensure_visible(cursor, wrap);
    }

    /// Returns the 0-based range of logical lines currently visible in the editor,
    /// given the total number of document lines.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    ///
    /// let view_state = EditorViewState::new();
    /// let range = view_state.visible_lines(100);
    /// assert_eq!(range.start, 0);
    /// ```
    pub fn visible_lines(&self, total_doc_lines: usize) -> std::ops::Range<usize> {
        self.scroller.visible_rows_range(total_doc_lines)
    }

    /// Checks whether the syntax parse has reached all currently visible lines in `state`.
    ///
    /// Returns `true` if the syntax parse is up to date, or `false` if more lines
    /// are still being lexed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorViewState;
    /// use texera_core::State;
    ///
    /// let view_state = EditorViewState::new();
    /// let state = State::from("fn main() {}");
    /// assert!(view_state.syntax_ready(&state));
    /// ```
    pub fn syntax_ready(&self, state: &texera_core::State) -> bool {
        let total_lines = state.doc().len_lines();
        let range = self.visible_lines(total_lines);
        if range.is_empty() {
            return true;
        }
        #[cfg(feature = "highlight")]
        if let Some(progress) = crate::plugins::highlight::HighlightPlugin::progress(state) {
            return progress.is_ready(range.end.saturating_sub(1));
        }
        true
    }
}
