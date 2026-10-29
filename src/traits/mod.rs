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

//! Extensible trait contracts defining the "Bring Your Own" (BYO) architecture.
//!
//! Everything in Texera is modular and decouples visual components through these traits:
//! - [`ViewPlugin`]: Base lifecycle hook for plugins reacting to state changes.
//! - [`Gutter`]: Side column components (e.g. line numbers).
//! - [`GutterMarker`]: Marker providers inside a gutter (e.g. git diff, dirty lines, breakpoints).
//! - [`Highlight`]: Highlighting engines (stateless or multi-line stateful parsing).
//! - [`DecorationProvider`]: Visual decorations applied over document text ranges.
//! - [`CommandHandler`]: Abstraction for dispatching edit commands and transactions.

use std::ops::Range;

use texera_core::TransactionSpec;
use ratatui::style::Style;
use texera_core::RangeSet;
use ratatui::text::Line;
use ratatui::text::Span;
use texera_core::State;

/// Context passed to view plugins when transactions are applied or the view updates.
///
/// # Examples
///
/// ```rust
/// use texera::ViewPlugin;
/// use texera::ViewUpdate;
///
/// struct MyPlugin;
///
/// impl ViewPlugin for MyPlugin {
///     fn update(&mut self, update: &ViewUpdate) {
///         if update.doc_changed {
///             // React to text modifications
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct ViewUpdate<'a> {
    /// The document state prior to the applied transaction.
    pub prev_state: &'a State,
    /// The current document state after the transaction.
    pub state: &'a State,
    /// Indicates whether viewport geometry or scrolling offset changed.
    pub viewport_changed: bool,
    /// Indicates whether document text contents or length changed.
    pub doc_changed: bool,
}

/// Base trait for modular visual extensions.
///
/// Implementors can register to receive state update notifications via [`ViewPlugin::update`].
pub trait ViewPlugin: Send + Sync {
    /// Notification hook triggered when the editor state or viewport changes.
    fn update(&mut self, _update: &ViewUpdate) {}
}

/// Trait for markers rendered inside a gutter column (e.g. dirty indicators, fold markers, breakpoints).
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use ratatui::text::Span;
/// use texera::GutterMarker;
///
/// struct BreakpointMarker;
///
/// impl GutterMarker for BreakpointMarker {
///     fn render_marker(&self, line_number: usize, _is_wrapped: bool) -> Option<Span<'static>> {
///         if line_number == 42 {
///             Some(Span::styled("●", Style::default().fg(Color::Red)))
///         } else {
///             None
///         }
///     }
/// }
/// ```
pub trait GutterMarker: Send + Sync {
    /// Returns a styled [`Span`] for the given 1-based line number, or `None` if no marker applies.
    fn render_marker(&self, line_number: usize, is_wrapped: bool) -> Option<Span<'static>>;
}

/// Trait for gutter components (line numbers, fold markers, status columns).
///
/// Gutters are attached to the left of the editor text area.
///
/// # Examples
///
/// ```rust
/// use ratatui::text::Line;
/// use ratatui::text::Span;
/// use texera::ViewPlugin;
/// use texera::Gutter;
///
/// struct SimpleGutter;
///
/// impl ViewPlugin for SimpleGutter {}
///
/// impl Gutter for SimpleGutter {
///     fn width(&self, _total_lines: usize) -> u16 {
///         4
///     }
///
///     fn render_line(&self, line_number: usize, _total_lines: usize, _is_wrapped: bool) -> Line<'static> {
///         Line::from(format!("{:>3} ", line_number))
///     }
/// }
/// ```
pub trait Gutter: ViewPlugin {
    /// Calculate gutter column width in terminal cells based on total document lines.
    fn width(&self, total_lines: usize) -> u16;

    /// Render a single line row in the gutter with fixed width based on total lines.
    fn render_line(&self, line_number: usize, total_lines: usize, is_wrapped: bool) -> Line<'static>;
}

/// Trait for syntax token highlighting engines.
///
/// Supports both lightweight stateless line highlight and incremental multi-line parse engines.
///
/// # Examples
///
/// ```rust
/// use std::ops::Range;
///
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera::ViewPlugin;
/// use texera::Highlight;
///
/// struct SimpleKeywordHighlighter;
///
/// impl ViewPlugin for SimpleKeywordHighlighter {}
///
/// impl Highlight for SimpleKeywordHighlighter {
///     fn highlight_line<'a>(&self, line_content: &'a str, _line_index: usize) -> Vec<(Range<usize>, Style)> {
///         let mut tokens = Vec::new();
///         if let Some(pos) = line_content.find("fn") {
///             tokens.push((pos..pos + 2, Style::default().fg(Color::Magenta)));
///         }
///         tokens
///     }
/// }
/// ```
pub trait Highlight: ViewPlugin {
    /// Tokenizes and returns styled ranges (char offsets inside the line) for a line slice,
    /// without any knowledge of the surrounding document.
    fn highlight_line<'a>(
        &self,
        line_content: &'a str,
        line_index: usize,
    ) -> Vec<(Range<usize>, Style)>;

    /// Called once per frame, before painting, with the inclusive range of logical lines that are
    /// about to be drawn. Stateful highlighters use it to advance their parse cache exactly as far
    /// as needed. The default implementation does nothing.
    fn prepare_viewport(&self, _state: &State, _visible_lines: std::ops::RangeInclusive<usize>) {}

    /// Highlights a whole logical line in the context of `state` (e.g. continuing a multi-line
    /// comment opened above). Returned ranges are char offsets inside `line_content`.
    /// The default implementation delegates to [`Highlight::highlight_line`].
    fn highlight_document_line(
        &self,
        _state: &State,
        line_index: usize,
        line_content: &str,
    ) -> Vec<(Range<usize>, Style)> {
        self.highlight_line(line_content, line_index)
    }
}

/// Custom decoration payload for styling or marking ranges.
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Modifier;
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera::Decoration;
///
/// let inline_mark = Decoration::Mark(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
/// let full_line = Decoration::Line(Style::default().bg(Color::DarkGray));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoration {
    /// Inline text style modification applied to a range of characters.
    Mark(Style),
    /// Full line style modification applied across the width of the row.
    Line(Style),
    /// Virtual replacement or tag embedded at the range.
    Widget(String, Style),
}

impl texera_core::RangeValue for Decoration {
    fn eq(&self, other: &Self) -> bool {
        self == other
    }

    fn map_mode(&self) -> texera_core::MapMode {
        texera_core::MapMode::TrackDel
    }
}

/// Provider of custom decorations overlaying the document text.
///
/// # Examples
///
/// ```rust
/// use std::ops::Range;
///
/// use texera_core::RangeSetBuilder;
/// use texera::DecorationProvider;
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera_core::RangeSet;
/// use texera::Decoration;
/// use texera::ViewPlugin;
/// use texera_core::State;
///
/// struct UnderlineProvider;
///
/// impl ViewPlugin for UnderlineProvider {}
///
/// impl DecorationProvider for UnderlineProvider {
///     fn decorations(&self, _state: &State, visible_range: Range<usize>) -> RangeSet<Decoration> {
///         let mut builder = RangeSetBuilder::new();
///         builder.add(visible_range.start, visible_range.end, Decoration::Mark(Style::default().fg(Color::Yellow)));
///         builder.finish()
///     }
/// }
/// ```
pub trait DecorationProvider: ViewPlugin {
    /// Produce a [`RangeSet`] of decorations covering the requested visible range.
    fn decorations(&self, state: &State, visible_range: Range<usize>) -> RangeSet<Decoration>;
}

/// Command handler for dispatching transactions and editing commands (Undo/Redo, etc.).
///
/// # Examples
///
/// ```rust
/// use texera_core::TransactionSpec;
/// use texera::CommandHandler;
/// use texera_core::State;
/// use texera_core::tr;
///
/// struct EditorController {
///     state: State,
/// }
///
/// impl CommandHandler for EditorController {
///     fn undo(&mut self) -> bool {
///         false
///     }
///     fn redo(&mut self) -> bool {
///         false
///     }
///     fn dispatch_transaction(&mut self, spec: TransactionSpec) -> Result<(), mynox::Error> {
///         self.state = self.state.clone().update(spec)?.into_state();
///         Ok(())
///     }
/// }
/// ```
pub trait CommandHandler {
    /// Undoes the last editing transaction. Returns `true` if successful.
    fn undo(&mut self) -> bool;
    /// Redoes the previously undone transaction. Returns `true` if successful.
    fn redo(&mut self) -> bool;
    /// Dispatches and applies a transactional update specification to the state.
    fn dispatch_transaction(&mut self, spec: TransactionSpec) -> Result<(), mynox::Error>;
}


/// Context passed to a [`GlyphTransformer`] to evaluate character rendering.
#[derive(Debug, Clone)]
pub struct GlyphContext<'a> {
    /// Character to be evaluated ('\t', ' ', etc.).
    pub ch: char,
    /// Width in terminal columns occupied by this glyph in layout.
    pub cell_width: usize,
    /// Visual column where the glyph starts (0-indexed).
    pub visual_col: usize,
    /// Absolute document character offset.
    pub doc_char_offset: usize,
    /// Logical line index (0-indexed).
    pub line_index: usize,
    /// Style resolved up to this point (base, syntax, and decorations).
    pub current_style: Style,
    /// Reference to the immutable document state.
    pub state: &'a State,
}

/// Visual replacement produced by a [`GlyphTransformer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformedGlyph {
    /// Single-character replacement with an optional style patch.
    Char(char, Option<Style>),
    /// Multi-cell replacement for tabs.
    ///
    /// Non-terminal cells use `filler`, and the final cell uses `terminator`.
    /// When `cell_width == 1`, only `terminator` is rendered.
    Tab {
        filler: char,
        terminator: char,
        style: Option<Style>,
    },
    /// Custom per-cell character sequence with an optional style patch.
    Cells(Vec<char>, Option<Style>),
}

/// Plugin trait for intercepting and transforming character glyphs and styles before rendering.
///
/// # Examples
///
/// ```rust
/// use texera::GlyphTransformer;
/// use texera::TransformedGlyph;
/// use texera::GlyphContext;
/// use ratatui::style::Style;
/// use texera::ViewPlugin;
///
/// struct SpaceDotTransformer;
///
/// impl ViewPlugin for SpaceDotTransformer {}
///
/// impl GlyphTransformer for SpaceDotTransformer {
///     fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
///         if ctx.ch == ' ' {
///             Some(TransformedGlyph::Char('·', None))
///         } else {
///             None
///         }
///     }
/// }
/// ```
pub trait GlyphTransformer: ViewPlugin {
    /// Evaluate whether the character at the given context should be visually replaced.
    ///
    /// Return `None` to retain default rendering.
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph>;
}
