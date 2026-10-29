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

//! Terminal input processing, key combination parsing, and action bindings.
//!
//! This module provides:
//! - [`EditorAction`]: High-level editor actions (typing, moving cursor, undo/redo).
//! - [`KeyCombination`]: Combination of keyboard keys and modifier flags (e.g. `ctrl-z`).
//! - [`Keymap`]: Trait for resolving terminal key events into editor actions.
//! - [`DefaultKeymap`]: Configurable keymap supporting JSON configuration files.
//! - [`apply_action`]: Helper function to apply an action directly to a document [`texera_core::State`].

pub mod action;
pub mod apply;
pub mod keymap;

pub use action::KeyCombination;
pub use action::EditorAction;

pub use keymap::DefaultKeymap;
pub use apply::apply_action;
pub use keymap::Keymap;

/// Convenience macro to load and parse a keymap JSON file embedded at compile-time.
///
/// # Examples
///
/// ```rust,ignore
/// use texera::include_keymap;
///
/// let keymap = include_keymap!("../assets/keymap.json").expect("valid keymap JSON");
/// ```
#[macro_export]
macro_rules! include_keymap {
    ($file:expr) => {
        $crate::DefaultKeymap::from_json_str(include_str!($file))
    };
}
