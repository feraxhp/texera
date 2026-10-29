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

//! Arbitrary text and line decorations overlay plugin.

use std::ops::Range;

use texera_core::RangeSetBuilder;
use ratatui::style::Style;
use texera_core::RangeSet;
use texera_core::State;

use crate::traits::DecorationProvider;
use crate::traits::Decoration;
use crate::traits::ViewPlugin;
use crate::traits::ViewUpdate;

/// Plugin for applying arbitrary custom decorations to the document.
///
/// Decorations can be character ranges (`Decoration::Mark`) or entire lines (`Decoration::Line`).
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera::CustomDecorationPlugin;
///
/// let plugin = CustomDecorationPlugin::empty()
///     .with_mark(0..5, Style::default().fg(Color::Yellow))
///     .with_line(10..20, Style::default().bg(Color::DarkGray));
/// ```
#[derive(Clone, Default)]
pub struct CustomDecorationPlugin {
    decorations: RangeSet<Decoration>,
}

impl CustomDecorationPlugin {
    /// Creates a decoration plugin pre-populated with the provided [`RangeSet`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CustomDecorationPlugin;
    /// use texera_core::RangeSet;
    ///
    /// let plugin = CustomDecorationPlugin::new(RangeSet::empty());
    /// ```
    pub fn new(decorations: RangeSet<Decoration>) -> Self {
        Self { decorations }
    }

    /// Creates an empty [`CustomDecorationPlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CustomDecorationPlugin;
    ///
    /// let plugin = CustomDecorationPlugin::empty();
    /// ```
    pub fn empty() -> Self {
        Self {
            decorations: RangeSet::empty(),
        }
    }

    /// Add an inline text styling decoration for the specified character range.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::CustomDecorationPlugin;
    ///
    /// let plugin = CustomDecorationPlugin::empty()
    ///     .with_mark(0..4, Style::default().fg(Color::Red));
    /// ```
    pub fn with_mark(mut self, range: Range<usize>, style: Style) -> Self {
        let mut builder = RangeSetBuilder::new();
        for r in self.decorations.iter() {
            builder.add(r.from, r.to, r.value.clone());
        }
        builder.add(range.start, range.end, Decoration::Mark(style));
        self.decorations = builder.finish();
        self
    }

    /// Add a full line styling decoration for the specified character range.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::CustomDecorationPlugin;
    ///
    /// let plugin = CustomDecorationPlugin::empty()
    ///     .with_line(0..10, Style::default().bg(Color::Blue));
    /// ```
    pub fn with_line(mut self, range: Range<usize>, style: Style) -> Self {
        let mut builder = RangeSetBuilder::new();
        for r in self.decorations.iter() {
            builder.add(r.from, r.to, r.value.clone());
        }
        builder.add(range.start, range.end, Decoration::Line(style));
        self.decorations = builder.finish();
        self
    }

    /// Replace the active decoration set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CustomDecorationPlugin;
    /// use texera_core::RangeSet;
    ///
    /// let mut plugin = CustomDecorationPlugin::empty();
    /// plugin.set_decorations(RangeSet::empty());
    /// ```
    pub fn set_decorations(&mut self, decorations: RangeSet<Decoration>) {
        self.decorations = decorations;
    }
}

impl ViewPlugin for CustomDecorationPlugin {
    fn update(&mut self, _update: &ViewUpdate) {}
}

impl DecorationProvider for CustomDecorationPlugin {
    fn decorations(&self, _state: &State, _visible_range: Range<usize>) -> RangeSet<Decoration> {
        self.decorations.clone()
    }
}
