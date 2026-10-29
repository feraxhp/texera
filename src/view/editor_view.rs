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

//! Builder definition and StatefulWidget implementation for the editor view.

use std::sync::Arc;

use ratatui::widgets::StatefulWidget;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use texera_core::State;

use crate::view::view_state::EditorViewState;
use crate::view::render::EditorThemeStyles;
use crate::view::render::render_editor;
use crate::traits::DecorationProvider;
use crate::traits::GlyphTransformer;
use crate::traits::ViewPlugin;
use crate::traits::Highlight;
use crate::traits::Gutter;

/// The central Ratatui [`StatefulWidget`] definition representing the visual code editor.
/// Follows a pure Opt-in / Builder pattern: zero overhead by default.
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use ratatui::style::Style;
/// use texera::LineNumberPlugin;
/// use texera::EditorView;
///
/// let editor = EditorView::new()
///     .with_wrap(true)
///     .with_gutter(LineNumberPlugin::new())
///     .with_style(Style::default().fg(Color::White));
/// ```
#[derive(Clone)]
pub struct EditorView {
    pub(crate) base_style: Style,
    pub(crate) wrap: bool,
    pub(crate) max_lines: Option<usize>,
    pub(crate) gutter: Option<Arc<dyn Gutter>>,
    pub(crate) highlighter: Option<Arc<dyn Highlight>>,
    pub(crate) decoration_providers: Vec<Arc<dyn DecorationProvider>>,
    pub(crate) plugins: Vec<Arc<dyn ViewPlugin>>,
    pub(crate) glyph_transformers: Vec<Arc<dyn GlyphTransformer>>,
    pub(crate) theme_styles: EditorThemeStyles,
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorView {
    /// Creates a completely blank, minimal editor view with default settings.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new();
    /// ```
    pub fn new() -> Self {
        Self {
            base_style: Style::default(),
            wrap: false,
            max_lines: None,
            gutter: None,
            highlighter: None,
            decoration_providers: Vec::new(),
            plugins: Vec::new(),
            glyph_transformers: Vec::new(),
            theme_styles: EditorThemeStyles::default(),
        }
    }

    /// Set base terminal style for unstyled editor text.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_style(Style::default().fg(Color::Green));
    /// ```
    pub fn with_style(mut self, style: Style) -> Self {
        self.base_style = style;
        self
    }

    /// Sets custom styles for primary/secondary cursors and selection.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::EditorThemeStyles;
    /// use texera::EditorView;
    ///
    /// let styles = EditorThemeStyles {
    ///     primary_cursor: Style::default().bg(Color::Cyan),
    ///     secondary_cursor: Style::default().bg(Color::Blue),
    ///     selection: Style::default().bg(Color::DarkGray),
    /// };
    /// let view = EditorView::new().with_theme_styles(styles);
    /// ```
    pub fn with_theme_styles(mut self, styles: EditorThemeStyles) -> Self {
        self.theme_styles = styles;
        self
    }

    /// Sets custom primary and secondary cursor styles.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use ratatui::style::Style;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new()
    ///     .with_cursor_styles(Style::default().bg(Color::Yellow), Style::default().bg(Color::DarkGray));
    /// ```
    pub fn with_cursor_styles(mut self, primary: Style, secondary: Style) -> Self {
        self.theme_styles.primary_cursor = primary;
        self.theme_styles.secondary_cursor = secondary;
        self
    }

    /// Enable or disable soft line wrapping.
    /// When disabled, horizontal scrolling is automatically enabled.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_wrap(true);
    /// ```
    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Limit the visible height / lines rendered by the widget.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_max_lines(20);
    /// ```
    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    /// Attach a line gutter component (line numbers, fold markers, etc.).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LineNumberPlugin;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_gutter(LineNumberPlugin::new());
    /// ```
    pub fn with_gutter<G: Gutter + 'static>(mut self, gutter: G) -> Self {
        self.gutter = Some(Arc::new(gutter));
        self
    }

    /// Attach a syntax highlighter.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_highlighter(HighlightPlugin::new());
    /// ```
    pub fn with_highlighter<H: Highlight + 'static>(mut self, highlighter: H) -> Self {
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Attach a custom decoration provider.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CustomDecorationPlugin;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_decoration_provider(CustomDecorationPlugin::empty());
    /// ```
    pub fn with_decoration_provider<D: DecorationProvider + 'static>(mut self, provider: D) -> Self {
        self.decoration_providers.push(Arc::new(provider));
        self
    }

    /// Attach a generic [`ViewPlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new().with_plugin(HistoryPlugin::new());
    /// ```
    pub fn with_plugin<P: ViewPlugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    /// Attach a custom glyph transformer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    ///
    /// let view = EditorView::new();
    /// ```
    pub fn with_glyph_transformer<T: GlyphTransformer + 'static>(mut self, transformer: T) -> Self {
        self.glyph_transformers.push(Arc::new(transformer));
        self
    }

    /// Bind the editor configuration to an active `texera_core::State` reference.
    ///
    /// Returns a [`BoundEditorView`] implementing [`StatefulWidget`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    /// use texera_core::State;
    ///
    /// let view = EditorView::new();
    /// let state = State::from("code");
    /// let bound = view.bind(&state);
    /// ```
    pub fn bind<'a>(&'a self, state: &'a State) -> BoundEditorView<'a> {
        BoundEditorView { view: self, state }
    }

    /// Convert into an owned [`EditorWidget`] bound to an active `State`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::EditorView;
    /// use texera_core::State;
    ///
    /// let state = State::from("code");
    /// let widget = EditorView::new().with_state(&state);
    /// ```
    pub fn with_state<'a>(self, state: &'a State) -> EditorWidget<'a> {
        EditorWidget { view: self, state }
    }

    /// Directly render into a Ratatui buffer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::buffer::Buffer;
    /// use ratatui::layout::Rect;
    /// use texera::EditorViewState;
    /// use texera::EditorView;
    /// use texera_core::State;
    ///
    /// let view = EditorView::new();
    /// let state = State::from("fn foo() {}");
    /// let area = Rect::new(0, 0, 30, 5);
    /// let mut buf = Buffer::empty(area);
    /// let mut view_state = EditorViewState::new();
    ///
    /// view.render_to_buffer(&state, area, &mut buf, &mut view_state);
    /// ```
    pub fn render_to_buffer(
        &self,
        state: &State,
        area: Rect,
        buf: &mut Buffer,
        view_state: &mut EditorViewState,
    ) {
        render_editor(self, state, area, buf, view_state);
    }
}

/// A borrowed view bound to an immutable document `State`, ready for Ratatui rendering.
pub struct BoundEditorView<'a> {
    /// Reference to the configured editor view.
    pub view: &'a EditorView,
    /// Reference to the active document state.
    pub state: &'a State,
}

impl<'a> StatefulWidget for BoundEditorView<'a> {
    type State = EditorViewState;

    fn render(self, area: Rect, buf: &mut Buffer, view_state: &mut Self::State) {
        render_editor(self.view, self.state, area, buf, view_state);
    }
}

/// An owned view bound to an immutable document `State`, ready for Ratatui rendering.
pub struct EditorWidget<'a> {
    /// Owned editor view configuration.
    pub view: EditorView,
    /// Reference to the active document state.
    pub state: &'a State,
}

impl<'a> StatefulWidget for EditorWidget<'a> {
    type State = EditorViewState;

    fn render(self, area: Rect, buf: &mut Buffer, view_state: &mut Self::State) {
        render_editor(&self.view, self.state, area, buf, view_state);
    }
}
