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

//! Gutter implementation displaying line numbers and marker columns.

use std::sync::Arc;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use crate::traits::{Gutter, GutterMarker, ViewPlugin, ViewUpdate};

/// Gutter plugin providing line numbers and optional attached markers.
///
/// Automatically scales width based on the number of digits in the document,
/// and accommodates any attached [`GutterMarker`] implementations (like dirty line indicators).
///
/// # Examples
///
/// ```rust
/// use ratatui::style::{Color, Style};
/// use texera::{LineNumberPlugin, UnsavedLinePlugin};
///
/// let gutter = LineNumberPlugin::new()
///     .with_style(Style::default().fg(Color::DarkGray))
///     .with_marker(UnsavedLinePlugin::new());
/// ```
#[derive(Clone)]
pub struct LineNumberPlugin {
    /// Style applied to the rendered line numbers.
    pub style: Style,
    /// Number of trailing space characters between line numbers and the editor text area.
    pub padding_right: usize,
    /// Attached marker providers rendered before line numbers.
    pub markers: Vec<Arc<dyn GutterMarker>>,
}

impl Default for LineNumberPlugin {
    fn default() -> Self {
        Self {
            style: Style::default(),
            padding_right: 1,
            markers: Vec::new(),
        }
    }
}

impl LineNumberPlugin {
    /// Creates a new default [`LineNumberPlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LineNumberPlugin;
    ///
    /// let gutter = LineNumberPlugin::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text style used for rendering line numbers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::{Color, Style};
    /// use texera::LineNumberPlugin;
    ///
    /// let gutter = LineNumberPlugin::new().with_style(Style::default().fg(Color::Blue));
    /// ```
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Attaches a [`GutterMarker`] provider to be rendered in the gutter column.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LineNumberPlugin, UnsavedLinePlugin};
    ///
    /// let gutter = LineNumberPlugin::new().with_marker(UnsavedLinePlugin::new());
    /// ```
    pub fn with_marker<M: GutterMarker + 'static>(mut self, marker: M) -> Self {
        self.markers.push(Arc::new(marker));
        self
    }

    /// Calculate the number of digits needed for line numbers.
    fn digits_for_lines(&self, total_lines: usize) -> usize {
        if total_lines == 0 {
            1
        } else {
            (total_lines as f64).log10().floor() as usize + 1
        }
    }
}

impl ViewPlugin for LineNumberPlugin {
    fn update(&mut self, _update: &ViewUpdate) {}
}

impl Gutter for LineNumberPlugin {
    fn width(&self, total_lines: usize) -> u16 {
        let digits = self.digits_for_lines(total_lines);
        let markers_width = self.markers.len();
        (digits + markers_width + self.padding_right) as u16
    }

    fn render_line(&self, line_number: usize, total_lines: usize, is_wrapped: bool) -> Line<'static> {
        let mut spans = Vec::new();
        let total_digits = self.digits_for_lines(total_lines);

        // 1. Render markers column (e.g. unsaved/dirty indicator in yellow) BEFORE line numbers
        for marker in &self.markers {
            if let Some(marker_span) = marker.render_marker(line_number, is_wrapped) {
                spans.push(marker_span);
            } else {
                spans.push(Span::raw(" "));
            }
        }

        // 2. Render logical line number (fixed width right-aligned according to largest line number)
        if is_wrapped {
            // Continuation line: blank space for digits
            spans.push(Span::raw(" ".repeat(total_digits)));
        } else {
            let content = format!("{:>width$}", line_number, width = total_digits);
            spans.push(Span::styled(content, self.style));
        }

        // 3. Padding right
        if self.padding_right > 0 {
            spans.push(Span::raw(" ".repeat(self.padding_right)));
        }

        Line::from(spans)
    }
}
