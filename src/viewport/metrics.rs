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

//! Terminal cell metrics calculation for Unicode characters, tabs, and graphemes.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

/// Computes visual terminal cell widths considering unicode glyphs and tab stops.
///
/// # Examples
///
/// ```rust
/// use texera::CellMetrics;
///
/// // Standard ASCII character width
/// assert_eq!(CellMetrics::char_width('a', 0, 4), 1);
///
/// // Tab character snapping to next tab stop (4 spaces)
/// assert_eq!(CellMetrics::char_width('\t', 0, 4), 4);
/// assert_eq!(CellMetrics::char_width('\t', 2, 4), 2);
///
/// // String visual width
/// assert_eq!(CellMetrics::string_width("hello\tworld", 0, 4), 13);
/// ```
pub struct CellMetrics;

impl CellMetrics {
    /// Calculate the visual column width of a character given the current visual column.
    ///
    /// Correctly handles tab stop alignment and wide Unicode characters.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CellMetrics;
    ///
    /// assert_eq!(CellMetrics::char_width('x', 0, 4), 1);
    /// assert_eq!(CellMetrics::char_width('\t', 1, 4), 3);
    /// ```
    #[inline]
    pub fn char_width(ch: char, current_col: usize, tab_size: usize) -> usize {
        match ch {
            '\t' => {
                let tab = if tab_size == 0 { 4 } else { tab_size };
                tab - (current_col % tab)
            }
            _ if ch.is_ascii() => 1,
            _ => UnicodeWidthChar::width(ch).unwrap_or(0),
        }
    }

    /// Calculate the visual column width of an entire string slice given the starting column.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CellMetrics;
    ///
    /// let width = CellMetrics::string_width("Rust 🦀", 0, 4);
    /// assert_eq!(width, 7); // "Rust " is 5, crab emoji is 2 cells
    /// ```
    pub fn string_width(text: &str, start_col: usize, tab_size: usize) -> usize {
        if !text.contains('\t') && text.is_ascii() { return text.len() }
        
        let mut col = start_col;
        for grapheme in text.graphemes(true) {
            for ch in grapheme.chars() {
                col += Self::char_width(ch, col, tab_size);
            }
        }
        col - start_col
    }

    /// Convert a character offset within a line to its visual terminal column position.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CellMetrics;
    ///
    /// let line = "\thello";
    /// assert_eq!(CellMetrics::char_index_to_column(line, 0, 4), 0);
    /// assert_eq!(CellMetrics::char_index_to_column(line, 1, 4), 4);
    /// ```
    pub fn char_index_to_column(line: &str, char_index: usize, tab_size: usize) -> usize {
        if !line.contains('\t') && line.is_ascii() {
            return char_index.min(line.len());
        }
        let mut col = 0;
        for (i, ch) in line.chars().enumerate() {
            if i >= char_index { break; }
            col += Self::char_width(ch, col, tab_size);
        }
        col
    }

    /// Convert a visual column position to the nearest character index in the line string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CellMetrics;
    ///
    /// let line = "\thello";
    /// assert_eq!(CellMetrics::column_to_char_index(line, 4, 4), 1);
    /// ```
    pub fn column_to_char_index(line: &str, target_col: usize, tab_size: usize) -> usize {
        if !line.contains('\t') && line.is_ascii() {
            return target_col.min(line.len());
        }
        let mut current_col = 0;
        for (i, ch) in line.chars().enumerate() {
            if current_col >= target_col { return i }
            
            let w = Self::char_width(ch, current_col, tab_size);
            if current_col + w > target_col {
                // If target falls inside a multi-cell character (e.g. wide CJK or tab),
                // snap to the character itself.
                return i;
            }
            current_col += w;
        }
        line.chars().count()
    }
}
