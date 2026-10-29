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

//! Visual editor widget components and rendering architecture.
//!
//! This module includes:
//! - [`EditorView`]: The primary builder struct to configure and assemble the editor widget.
//! - [`BoundEditorView`] & [`EditorWidget`]: State-bound view wrappers implementing Ratatui's `StatefulWidget`.
//! - [`EditorViewState`]: Persistent viewport and scrolling state across frames.
//! - [`EditorThemeStyles`]: Styling specifications for cursors and selections.
//! - [`render_editor`]: Low-level rendering pipeline.

/// Editor view builder and StatefulWidget widget wrappers.
pub mod editor_view;
/// Low-level rendering pipeline.
pub mod render;
/// View state preserving scroll offsets across frames.
pub mod view_state;

pub use editor_view::BoundEditorView;
pub use editor_view::EditorWidget;
pub use editor_view::EditorView;

pub use render::EditorThemeStyles;
pub use render::render_editor;

pub use view_state::EditorViewState;
