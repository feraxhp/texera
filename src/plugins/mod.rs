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

//! Built-in plugins for syntax highlighting, gutter markers, history, and decorations.
//!
//! Available plugins:
//! - [`HighlightPlugin`]: Incremental syntax highlighting with declarative TOML state machines.
//! - [`LineNumberPlugin`]: Dynamic-width gutter with line numbers and optional markers.
//! - [`UnsavedLinePlugin`]: Dirty/modified lines tracking using Myers diff.
//! - [`HistoryPlugin`]: Transactional undo/redo integration.
//! - [`CustomDecorationPlugin`]: Inline marks and line decorations.

pub mod decorations;
pub mod gutter;
#[cfg(feature = "highlight")]
pub mod highlight;
pub mod history;
pub mod unsaved_lines;
#[cfg(feature = "whitespace")]
pub mod whitespace;

pub use decorations::CustomDecorationPlugin;
pub use unsaved_lines::UnsavedLinePlugin;
pub use gutter::LineNumberPlugin;

pub use history::HistoryCommander;
pub use history::HistoryPlugin;
#[cfg(feature = "whitespace")]
pub use whitespace::WhitespacePlugin;

#[cfg(feature = "highlight")]
pub use {
    highlight::LanguageRegistry, 
    highlight::CompiledLanguage, 
    highlight::HighlightPlugin, 
    highlight::SyntaxProgress, 
    highlight::LanguageToml,
    highlight::LanguageSpec,
    highlight::SyntaxField, 
    highlight::SyntaxError, 
    highlight::SyntaxCache, 
    highlight::SyntaxStats, 
    highlight::StateFrame, 
    highlight::Registered, 
    highlight::ScopedToken,
    highlight::TokenName,
    highlight::ThemeMap, 
    highlight::LexState, 
    highlight::Loader, 
    highlight::LangId, 
    
    highlight::MAX_POOL_STATES,
    highlight::MAX_DEPTH, 

    highlight::dir_loader, 
    highlight::dir_spec_loader,
};
