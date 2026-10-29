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

//! Visual line representation, word wrapping, and coordinate mapping.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use texera_core::Text;

use crate::viewport::metrics::CellMetrics;

/// Represents a single visual line rendered on the terminal.
/// When wrapping is enabled, a single logical document line may be split into multiple visual lines.
///
/// # Examples
///
/// ```rust
/// use texera::VisualLine;
///
/// let line = VisualLine {
///     logical_line_index: 0,
///     is_wrapped_continuation: false,
///     char_range: 0..5,
///     text: "hello".to_string(),
///     visual_width: 5,
/// };
/// assert_eq!(line.logical_line_index, 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualLine {
    /// 0-based logical line index in the document.
    pub logical_line_index: usize,
    /// Indicates whether this visual line is a soft-wrapped continuation of the preceding line.
    pub is_wrapped_continuation: bool,
    /// Absolute character range in the document corresponding to this visual line.
    pub char_range: Range<usize>,
    /// Text content for this visual slice.
    pub text: String,
    /// Visual column width in terminal cells.
    pub visual_width: usize,
}

/// Coordinate of a point on the visual grid (row and column).
///
/// # Examples
///
/// ```rust
/// use texera::VisualPosition;
///
/// let pos = VisualPosition { visual_row: 10, visual_col: 4 };
/// assert_eq!(pos.visual_row, 10);
/// assert_eq!(pos.visual_col, 4);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VisualPosition {
    /// 0-based visual row index.
    pub visual_row: usize,
    /// 0-based visual terminal column.
    pub visual_col: usize,
}

/// Compute all visual lines for a document based on wrapping settings and viewport width.
///
/// # Examples
///
/// ```rust
/// use texera::compute_visual_lines;
/// use texera_core::Text;
///
/// let doc = Text::from("line 1\nline 2");
/// let visual_lines = compute_visual_lines(&doc, false, 80, 4);
/// assert_eq!(visual_lines.len(), 2);
/// ```
pub fn compute_visual_lines(
    doc: &Text,
    wrap: bool,
    available_width: usize,
    tab_size: usize,
) -> Vec<VisualLine> {
    let mut visual_lines = Vec::with_capacity(doc.len_lines());
    let width = if available_width == 0 { 1 } else { available_width };

    for line in doc.iter_lines(None, None) {
        let from = line.from();
        let to_trimmed = line.to_trimmed();
        let line_str = line.text_trimmed().into_owned();

        if !wrap {
            let vis_width = CellMetrics::string_width(&line_str, 0, tab_size);
            visual_lines.push(VisualLine {
                logical_line_index: line.number(),
                is_wrapped_continuation: false,
                char_range: from..to_trimmed,
                text: line_str,
                visual_width: vis_width,
            });
        } else {
            let wrapped = wrap_single_line(
                &line_str,
                from,
                line.number(),
                width,
                tab_size,
            );
            visual_lines.extend(wrapped);
        }
    }

    if visual_lines.is_empty() {
        visual_lines.push(VisualLine {
            logical_line_index: 0,
            is_wrapped_continuation: false,
            char_range: 0..0,
            text: String::new(),
            visual_width: 0,
        });
    }

    visual_lines
}

/// Wraps a single logical line string into multiple visual line segments.
///
/// # Examples
///
/// ```rust
/// use texera::wrap_single_line;
///
/// let segments = wrap_single_line("hello world from rust", 0, 0, 12, 4);
/// assert!(segments.len() >= 2);
/// ```
pub fn wrap_single_line(
    line_text: &str,
    line_from: usize,
    logical_line_index: usize,
    available_width: usize,
    tab_size: usize,
) -> Vec<VisualLine> {
    if line_text.is_empty() {
        return vec![VisualLine {
            logical_line_index,
            is_wrapped_continuation: false,
            char_range: line_from..line_from,
            text: String::new(),
            visual_width: 0,
        }];
    }

    let mut result = Vec::new();
    let mut current_segment = String::new();
    let mut current_col = 0;
    let mut segment_start_char = 0;
    let mut current_char_offset = 0;
    let mut is_continuation = false;

    for word in line_text.split_word_bounds() {
        let word_width = CellMetrics::string_width(word, current_col, tab_size);
        let word_char_count = word.chars().count();

        if current_col + word_width <= available_width || current_segment.is_empty() {
            // Word fits on current line, or line is empty and word must be placed
            if current_segment.is_empty() && word_width > available_width {
                // Single word exceeds line width: split by graphemes
                for grapheme in word.graphemes(true) {
                    let g_width = CellMetrics::string_width(grapheme, current_col, tab_size);
                    let g_chars = grapheme.chars().count();

                    if current_col + g_width > available_width && !current_segment.is_empty() {
                        result.push(VisualLine {
                            logical_line_index,
                            is_wrapped_continuation: is_continuation,
                            char_range: (line_from + segment_start_char)
                                ..(line_from + current_char_offset),
                            text: current_segment.clone(),
                            visual_width: current_col,
                        });
                        is_continuation = true;
                        current_segment.clear();
                        current_col = 0;
                        segment_start_char = current_char_offset;
                    }

                    current_segment.push_str(grapheme);
                    current_col += g_width;
                    current_char_offset += g_chars;
                }
            } else {
                current_segment.push_str(word);
                current_col += word_width;
                current_char_offset += word_char_count;
            }
        } else {
            // Flush current segment and start a new visual line
            result.push(VisualLine {
                logical_line_index,
                is_wrapped_continuation: is_continuation,
                char_range: (line_from + segment_start_char)
                    ..(line_from + current_char_offset),
                text: current_segment.clone(),
                visual_width: current_col,
            });
            is_continuation = true;
            current_segment.clear();
            current_col = 0;
            segment_start_char = current_char_offset;

            // Now handle the word on the fresh line
            if word_width > available_width {
                for grapheme in word.graphemes(true) {
                    let g_width = CellMetrics::string_width(grapheme, current_col, tab_size);
                    let g_chars = grapheme.chars().count();

                    if current_col + g_width > available_width && !current_segment.is_empty() {
                        result.push(VisualLine {
                            logical_line_index,
                            is_wrapped_continuation: is_continuation,
                            char_range: (line_from + segment_start_char)
                                ..(line_from + current_char_offset),
                            text: current_segment.clone(),
                            visual_width: current_col,
                        });
                        current_segment.clear();
                        current_col = 0;
                        segment_start_char = current_char_offset;
                    }

                    current_segment.push_str(grapheme);
                    current_col += g_width;
                    current_char_offset += g_chars;
                }
            } else {
                current_segment.push_str(word);
                current_col += word_width;
                current_char_offset += word_char_count;
            }
        }
    }

    if !current_segment.is_empty() {
        result.push(VisualLine {
            logical_line_index,
            is_wrapped_continuation: is_continuation,
            char_range: (line_from + segment_start_char)..(line_from + current_char_offset),
            text: current_segment,
            visual_width: current_col,
        });
    }

    result
}

/// Convert an absolute document character offset to visual coordinates (row, col).
///
/// # Examples
///
/// ```rust
/// use texera::doc_offset_to_visual_pos;
/// use texera::compute_visual_lines;
/// use texera_core::Text;
///
/// let doc = Text::from("hello\nworld");
/// let visual_lines = compute_visual_lines(&doc, false, 80, 4);
/// let pos = doc_offset_to_visual_pos(&visual_lines, 7, 4);
/// assert_eq!(pos.visual_row, 1);
/// assert_eq!(pos.visual_col, 1);
/// ```
pub fn doc_offset_to_visual_pos(
    visual_lines: &[VisualLine],
    doc_offset: usize,
    tab_size: usize,
) -> VisualPosition {
    if visual_lines.is_empty() {
        return VisualPosition::default();
    }

    // Fast binary search to find the visual line containing doc_offset
    let idx = visual_lines.partition_point(|vline| vline.char_range.end < doc_offset);
    if idx < visual_lines.len() {
        let vline = &visual_lines[idx];
        if doc_offset >= vline.char_range.start && doc_offset <= vline.char_range.end {
            let char_in_segment = doc_offset - vline.char_range.start;
            let visual_col =
                CellMetrics::char_index_to_column(&vline.text, char_in_segment, tab_size);
            return VisualPosition {
                visual_row: idx,
                visual_col,
            };
        }
    }

    // If offset is past the end of the last line, place at the end of the last visual line
    let last_idx = visual_lines.len() - 1;
    let last_line = &visual_lines[last_idx];
    VisualPosition {
        visual_row: last_idx,
        visual_col: last_line.visual_width,
    }
}

/// Convert visual coordinates back to absolute document character offset.
///
/// # Examples
///
/// ```rust
/// use texera::visual_pos_to_doc_offset;
/// use texera::compute_visual_lines;
/// use texera_core::Text;
///
/// let doc = Text::from("hello\nworld");
/// let visual_lines = compute_visual_lines(&doc, false, 80, 4);
/// let offset = visual_pos_to_doc_offset(&visual_lines, 1, 1, 4);
/// assert_eq!(offset, 7);
/// ```
pub fn visual_pos_to_doc_offset(
    visual_lines: &[VisualLine],
    visual_row: usize,
    visual_col: usize,
    tab_size: usize,
) -> usize {
    if visual_lines.is_empty() {
        return 0;
    }

    let clamped_row = visual_row.min(visual_lines.len() - 1);
    let vline = &visual_lines[clamped_row];

    let char_index = CellMetrics::column_to_char_index(&vline.text, visual_col, tab_size);
    vline.char_range.start + char_index
}
