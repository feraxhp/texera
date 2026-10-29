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

//! Progress and statistics reporting for incremental syntax highlighting.

/// How far the lexical parse has advanced for a given document.
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxProgress;
///
/// let progress = SyntaxProgress { lines_ready: 50, total_lines: 100, };
/// assert!(progress.is_ready(10));
/// assert!(!progress.is_ready(60));
/// assert_eq!(progress.ratio(), 0.5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxProgress {
    /// Number of leading lines `0..lines_ready` whose entry state is known, i.e. the lines that
    /// can be highlighted exactly.
    pub lines_ready: usize,
    /// Total number of lines of the document.
    pub total_lines: usize,
}

impl SyntaxProgress {
    /// Whether the entry state of `line` (0-based) is known.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 5, total_lines: 10, };
    /// assert!(p.is_ready(4));
    /// assert!(!p.is_ready(5));
    /// ```
    pub fn is_ready(&self, line: usize) -> bool {
        line < self.lines_ready
    }

    /// Last line (0-based) that can be highlighted exactly, if any.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 5, total_lines: 10, };
    /// assert_eq!(p.last_ready_line(), Some(4));
    /// ```
    pub fn last_ready_line(&self) -> Option<usize> {
        self.lines_ready.checked_sub(1)
    }

    /// Whether the whole document has been parsed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 10, total_lines: 10, };
    /// assert!(p.is_complete());
    /// ```
    pub fn is_complete(&self) -> bool {
        self.lines_ready >= self.total_lines
    }

    /// Fraction of the document that is ready, in `0.0..=1.0`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 25, total_lines: 100, };
    /// assert!((p.ratio() - 0.25).abs() < f32::EPSILON);
    /// ```
    pub fn ratio(&self) -> f32 {
        if self.total_lines == 0 {
            1.0
        } else {
            self.lines_ready as f32 / self.total_lines as f32
        }
    }
}

/// Counters describing the work done by a cache (inherited through transactions).
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxStats;
///
/// let stats = SyntaxStats::default();
/// assert_eq!(stats.lines_lexed, 0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyntaxStats {
    /// Total number of lines lexed to compute entry states.
    pub lines_lexed: u64,
    /// Number of transactions that discarded already-known entry states.
    pub invalidations: u64,
    /// Number of times an edit was absorbed by restoring old entries (resynchronisation).
    pub resyncs: u64,
}
