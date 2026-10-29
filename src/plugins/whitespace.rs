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

//! Whitespace visualization plugin for rendering spaces and tabs with custom glyphs.

use ratatui::style::Style;

use crate::traits::GlyphTransformer;
use crate::traits::TransformedGlyph;
use crate::traits::GlyphContext;
use crate::traits::ViewPlugin;

/// Plugin that replaces whitespace characters (spaces and tabs) with customizable visual glyphs.
///
/// Default glyphs:
/// - Space: `'·'` (middle dot)
/// - Tab filler: `' '` (space)
/// - Tab terminator: `'⇥'` (tab symbol)
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera::WhitespacePlugin;
/// use texera::EditorView;
///
/// let plugin = WhitespacePlugin::default()
///     .with_style(Style::default().fg(Color::DarkGray));
///
/// let view = EditorView::new().with_glyph_transformer(plugin);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhitespacePlugin {
    /// Glyph used to represent space characters.
    pub space_char: char,
    /// Filler character used across non-terminal tab columns.
    pub tab_filler: char,
    /// Terminator character used at the final column of a tab span.
    pub tab_terminator: char,
    /// Optional style applied to rendered whitespace glyphs.
    pub style: Option<Style>,
}

impl Default for WhitespacePlugin {
    fn default() -> Self {
        Self {
            space_char: '·',
            tab_filler: ' ',
            tab_terminator: '⇥',
            style: None,
        }
    }
}

impl WhitespacePlugin {
    /// Creates a new whitespace plugin with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the character used for space visualization.
    pub fn with_space_char(mut self, ch: char) -> Self {
        self.space_char = ch;
        self
    }

    /// Sets the filler character used for intermediate tab columns.
    pub fn with_tab_filler(mut self, ch: char) -> Self {
        self.tab_filler = ch;
        self
    }

    /// Sets the terminator character used for the final tab column.
    pub fn with_tab_terminator(mut self, ch: char) -> Self {
        self.tab_terminator = ch;
        self
    }

    /// Sets both the filler and terminator characters for tabs.
    pub fn with_tab_chars(mut self, filler: char, terminator: char) -> Self {
        self.tab_filler = filler;
        self.tab_terminator = terminator;
        self
    }

    /// Sets an optional style for rendered whitespace glyphs.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

impl ViewPlugin for WhitespacePlugin {}

impl GlyphTransformer for WhitespacePlugin {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        match ctx.ch {
            ' ' => Some(TransformedGlyph::Char(self.space_char, self.style)),
            '\t' => Some(TransformedGlyph::Tab {
                filler: self.tab_filler,
                terminator: self.tab_terminator,
                style: self.style,
            }),
            _ => None,
        }
    }
}
