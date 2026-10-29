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

//! History plugin wrapping undo and redo capabilities.

use texera_pluggins::HistoryConfig;
use texera_core::TransactionSpec;
use texera_pluggins::redo_depth;
use texera_pluggins::redo_state;
use texera_pluggins::undo_depth;
use texera_pluggins::undo_state;
use texera_pluggins::history;
use texera_core::Extension;
use texera_core::State;

use crate::traits::CommandHandler;
use crate::traits::ViewPlugin;
use crate::traits::ViewUpdate;

/// Plugin wrapping history operations (undo/redo) from `texera-pluggins`.
///
/// # Examples
///
/// ```rust
/// use texera::HistoryPlugin;
/// use texera_core::State;
///
/// let mut state = State::from("hello").with_extensions([
///     HistoryPlugin::extension(None),
/// ]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct HistoryPlugin;

impl HistoryPlugin {
    /// Creates a new [`HistoryPlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    ///
    /// let plugin = HistoryPlugin::new();
    /// ```
    pub fn new() -> Self {
        Self
    }

    /// Creates a default [`Extension`] configuring history tracking for a [`State`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let state = State::from("code").with_extensions([
    ///     HistoryPlugin::extension(None),
    /// ]);
    /// ```
    pub fn extension(config: Option<HistoryConfig>) -> Extension {
        history(config.unwrap_or_default())
    }

    /// Performs an undo operation on the provided state.
    ///
    /// Returns `true` if an undo step was performed, or `false` if the undo stack was empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let mut state = State::from("test").with_extensions([HistoryPlugin::extension(None)]);
    /// assert!(!HistoryPlugin::undo(&mut state));
    /// ```
    pub fn undo(state: &mut State) -> bool {
        undo_state(state)
    }

    /// Performs a redo operation on the provided state.
    ///
    /// Returns `true` if a redo step was performed, or `false` if the redo stack was empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let mut state = State::from("test").with_extensions([HistoryPlugin::extension(None)]);
    /// assert!(!HistoryPlugin::redo(&mut state));
    /// ```
    pub fn redo(state: &mut State) -> bool {
        redo_state(state)
    }

    /// Returns the current depth of the undo stack.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let state = State::from("test").with_extensions([HistoryPlugin::extension(None)]);
    /// assert_eq!(HistoryPlugin::undo_depth(&state), 0);
    /// ```
    pub fn undo_depth(state: &State) -> usize {
        undo_depth(state)
    }

    /// Returns the current depth of the redo stack.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let state = State::from("test").with_extensions([HistoryPlugin::extension(None)]);
    /// assert_eq!(HistoryPlugin::redo_depth(&state), 0);
    /// ```
    pub fn redo_depth(state: &State) -> usize {
        redo_depth(state)
    }

    /// Wrap a mutable state reference into a [`CommandHandler`] to execute undo/redo and transactions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::CommandHandler;
    /// use texera::HistoryPlugin;
    /// use texera_core::State;
    ///
    /// let mut state = State::from("test").with_extensions([HistoryPlugin::extension(None)]);
    /// let plugin = HistoryPlugin::new();
    /// let mut cmd = plugin.commander(&mut state);
    /// assert!(!cmd.undo());
    /// ```
    pub fn commander<'a>(&self, state: &'a mut State) -> HistoryCommander<'a> {
        HistoryCommander { state }
    }
}

impl ViewPlugin for HistoryPlugin {
    fn update(&mut self, _update: &ViewUpdate) {}
}

/// Helper executing commands and transactions with history capabilities.
pub struct HistoryCommander<'a> {
    /// Mutable reference to the underlying document state.
    pub state: &'a mut State,
}

impl<'a> CommandHandler for HistoryCommander<'a> {
    fn undo(&mut self) -> bool {
        undo_state(self.state)
    }

    fn redo(&mut self) -> bool {
        redo_state(self.state)
    }

    fn dispatch_transaction(&mut self, spec: TransactionSpec) -> Result<(), mynox::Error> {
        let next_state = self.state.clone().update(spec)?.into_state();
        *self.state = next_state;
        Ok(())
    }
}
