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

//! Viewport layout, metrics calculation, line wrapping, and scrolling.
//!
//! This module includes:
//! - [`CellMetrics`]: Grapheme and tab-stop visual cell width computations.
//! - [`Scroller`]: Viewport bounds controller with horizontal and vertical autoscroll.
//! - [`VisualLine`]: Terminal rendered line representation with wrap continuation flags.
//! - [`VisualPosition`]: Coordinate pair (`visual_row`, `visual_col`) on the visual grid.
//! - [`compute_visual_lines`]: Generator for visual lines from document [`texera_core::Text`].
//! - [`doc_offset_to_visual_pos`] & [`visual_pos_to_doc_offset`]: Coordinate converters.

/// Grapheme and cell metrics.
pub mod metrics;
/// Viewport scrolling controller.
pub mod scroller;
/// Visual line segmentation and coordinate transformations.
pub mod visual_line;

pub use metrics::CellMetrics;
pub use scroller::Scroller;
pub use visual_line::{
    compute_visual_lines, doc_offset_to_visual_pos, visual_pos_to_doc_offset, wrap_single_line,
    VisualLine, VisualPosition,
};
