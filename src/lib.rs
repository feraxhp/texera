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

//! # Texera
//!
//! A modular code-editor widget for [Ratatui](https://github.com/ratatui/ratatui),
//! following the **"Bring Your Own" (BYO)** trait-based philosophy.
//!
//! Built on top of the immutable transactional engine of `texera-core` based on CodeMirror 6.
//!
//! ## Key Features
//!
//! - **Decoupled & Trait-Based**: Opt-in architecture with zero overhead for unused features.
//! - **Ratatui Native**: Implements [`ratatui::widgets::StatefulWidget`] via [`EditorView`].
//! - **Transactional State**: Built on immutable document trees with transaction history, selections, and undo/redo.
//! - **Incremental Syntax Highlighting**: Declarative TOML Pushdown Automaton state machines with per-document caching.
//! - **Multi-cursor & Word Wrap**: Native multiple cursors, soft wrapping, and bidirectional coordinate mapping.
//!
//! ## Basic Usage
//!
//! ```rust
//! use texera::EditorViewState;
//! use texera::DefaultKeymap;
//! use texera::EditorAction;
//! use texera::apply_action;
//! use texera::EditorView;
//!
//! use texera_core::State;
//!
//! // 1. Create document state
//! let mut state = State::from("fn main() {\n    println!(\"Hello Texera\");\n}");
//!
//! // 2. Create editor view widget and persistent view state
//! let view = EditorView::new().with_wrap(true);
//! let mut view_state = EditorViewState::new();
//!
//! // 3. Handle actions
//! apply_action(&mut state, EditorAction::MoveDocEnd, None).unwrap();
//! ```

pub mod input;
pub mod plugins;
pub mod traits;
pub mod view;
pub mod viewport;

pub use input::KeyCombination;
pub use input::DefaultKeymap;
pub use input::EditorAction;
pub use input::apply_action;
pub use input::Keymap;

pub use plugins::CustomDecorationPlugin;
pub use plugins::UnsavedLinePlugin;
pub use plugins::HistoryCommander;
pub use plugins::LineNumberPlugin;
#[cfg(feature = "whitespace")]
pub use plugins::WhitespacePlugin;
pub use plugins::HistoryPlugin;

pub use traits::DecorationProvider;
pub use traits::GlyphTransformer;
pub use traits::TransformedGlyph;
pub use traits::CommandHandler;
pub use traits::GlyphContext;
pub use traits::GutterMarker;
pub use traits::Decoration;
pub use traits::ViewPlugin;
pub use traits::ViewUpdate;
pub use traits::Highlight;
pub use traits::Gutter;

pub use view::EditorThemeStyles;
pub use view::BoundEditorView;
pub use view::EditorViewState;
pub use view::render_editor;
pub use view::EditorWidget;
pub use view::EditorView;

pub use viewport::doc_offset_to_visual_pos;
pub use viewport::visual_pos_to_doc_offset;
pub use viewport::compute_visual_lines;
pub use viewport::wrap_single_line;
pub use viewport::VisualPosition;
pub use viewport::CellMetrics;
pub use viewport::VisualLine;
pub use viewport::Scroller;

#[cfg(feature = "highlight")]
pub use {
    texera_macros::include_languaje_dir,

    plugins::LanguageRegistry, 
    plugins::CompiledLanguage, 
    plugins::HighlightPlugin, 
    plugins::SyntaxProgress, 
    plugins::LanguageToml,
    plugins::LanguageSpec,
    plugins::SyntaxField, 
    plugins::SyntaxError, 
    plugins::SyntaxCache, 
    plugins::SyntaxStats, 
    plugins::StateFrame, 
    plugins::Registered, 
    plugins::ScopedToken,
    plugins::TokenName,
    plugins::ThemeMap, 
    plugins::LexState, 
    plugins::Loader, 
    plugins::LangId, 
    
    plugins::MAX_POOL_STATES,
    plugins::MAX_DEPTH, 

    plugins::dir_loader, 
    plugins::dir_spec_loader,
};
