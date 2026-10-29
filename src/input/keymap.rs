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

//! Keymap handling and action dispatching.

use std::collections::HashMap;
use std::str::FromStr;
use std::path::Path;
use std::fs;

use crossterm::event::KeyModifiers;
use crossterm::event::KeyEvent;
use crossterm::event::KeyCode;

pub use crate::input::apply::apply_action;
use crate::input::action::KeyCombination;
use crate::input::action::EditorAction;

/// Trait resolving terminal [`KeyEvent`]s to [`EditorAction`]s.
///
/// Implement this trait to provide custom keybinding logic or custom keymap profiles.
///
/// # Examples
///
/// ```rust
/// use crossterm::event::KeyModifiers;
/// use crossterm::event::KeyEvent;
/// use crossterm::event::KeyCode;
/// use texera::EditorAction;
/// use texera::Keymap;
///
/// struct MinimalKeymap;
///
/// impl Keymap for MinimalKeymap {
///     fn action_for_key(&self, event: &KeyEvent) -> Option<EditorAction> {
///         match (event.code, event.modifiers) {
///             (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(EditorAction::Undo),
///             _ => None,
///         }
///     }
/// }
/// ```
pub trait Keymap: Send + Sync {
    /// Resolves a terminal key event into an optional [`EditorAction`].
    fn action_for_key(&self, event: &KeyEvent) -> Option<EditorAction>;
}

/// Standard configurable keymap supporting default bindings and custom JSON mappings.
///
/// # Default Bindings
/// - `Up`, `Down`, `Left`, `Right`: Standard cursor movement.
/// - `Home`, `End`: Move to line start / end.
/// - `Shift + Movement`: Extend selection.
/// - `Backspace`, `Delete`: Character deletion.
/// - `Enter`, `Tab`: Character insertion.
/// - `Ctrl-Z`, `Ctrl-Y`, `Ctrl-Shift-Z`: Undo / Redo.
/// - `Alt-Up`, `Alt-Down`: Add cursor above / below (multi-cursor).
/// - `Ctrl-A`: Select entire document.
///
/// # Examples
///
/// ```rust
/// use crossterm::event::KeyModifiers;
/// use crossterm::event::KeyEvent;
/// use crossterm::event::KeyCode;
/// use texera::DefaultKeymap;
/// use texera::EditorAction;
/// use texera::Keymap;
///
/// let keymap = DefaultKeymap::new();
/// let event = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
/// assert_eq!(keymap.action_for_key(&event), Some(EditorAction::Undo));
/// ```
#[derive(Debug, Clone)]
pub struct DefaultKeymap {
    /// Lookup table mapping [`KeyCombination`] to [`EditorAction`].
    pub bindings: HashMap<KeyCombination, EditorAction>,
}

impl Default for DefaultKeymap {
    fn default() -> Self {
        let mut map = HashMap::new();

        // Editing
        map.insert(KeyCombination::new(KeyCode::Backspace, KeyModifiers::empty()), EditorAction::DeleteBackward);
        map.insert(KeyCombination::new(KeyCode::Delete, KeyModifiers::empty()), EditorAction::DeleteForward);
        map.insert(KeyCombination::new(KeyCode::Enter, KeyModifiers::empty()), EditorAction::InsertNewline);
        map.insert(KeyCombination::new(KeyCode::Tab, KeyModifiers::empty()), EditorAction::InsertTab);

        // Navigation
        map.insert(KeyCombination::new(KeyCode::Left, KeyModifiers::empty()), EditorAction::MoveLeft);
        map.insert(KeyCombination::new(KeyCode::Right, KeyModifiers::empty()), EditorAction::MoveRight);
        map.insert(KeyCombination::new(KeyCode::Up, KeyModifiers::empty()), EditorAction::MoveUp);
        map.insert(KeyCombination::new(KeyCode::Down, KeyModifiers::empty()), EditorAction::MoveDown);
        map.insert(KeyCombination::new(KeyCode::Home, KeyModifiers::empty()), EditorAction::MoveLineStart);
        map.insert(KeyCombination::new(KeyCode::End, KeyModifiers::empty()), EditorAction::MoveLineEnd);

        // Selection (with Shift)
        map.insert(KeyCombination::new(KeyCode::Left, KeyModifiers::SHIFT), EditorAction::SelectLeft);
        map.insert(KeyCombination::new(KeyCode::Right, KeyModifiers::SHIFT), EditorAction::SelectRight);
        map.insert(KeyCombination::new(KeyCode::Up, KeyModifiers::SHIFT), EditorAction::SelectUp);
        map.insert(KeyCombination::new(KeyCode::Down, KeyModifiers::SHIFT), EditorAction::SelectDown);
        map.insert(KeyCombination::new(KeyCode::Home, KeyModifiers::SHIFT), EditorAction::SelectLineStart);
        map.insert(KeyCombination::new(KeyCode::End, KeyModifiers::SHIFT), EditorAction::SelectLineEnd);

        // History
        map.insert(KeyCombination::new(KeyCode::Char('z'), KeyModifiers::CONTROL), EditorAction::Undo);
        map.insert(KeyCombination::new(KeyCode::Char('y'), KeyModifiers::CONTROL), EditorAction::Redo);
        map.insert(KeyCombination::new(KeyCode::Char('z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT), EditorAction::Redo);

        // Multi-cursor
        map.insert(KeyCombination::new(KeyCode::Down, KeyModifiers::ALT), EditorAction::AddCursorBelow);
        map.insert(KeyCombination::new(KeyCode::Up, KeyModifiers::ALT), EditorAction::AddCursorAbove);

        // Selection shortcuts
        map.insert(KeyCombination::new(KeyCode::Char('a'), KeyModifiers::CONTROL), EditorAction::SelectAll);

        Self { bindings: map, }
    }
}

impl DefaultKeymap {
    /// Creates a new [`DefaultKeymap`] with standard defaults (arrows, backspace, enter, undo/redo).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::DefaultKeymap;
    ///
    /// let keymap = DefaultKeymap::new();
    /// assert!(!keymap.bindings.is_empty());
    /// ```
    pub fn new() -> Self { Self::default() }

    /// Load or extend keybindings from a JSON string mapping `"key-combo": "action_name"`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crossterm::event::KeyModifiers;
    /// use crossterm::event::KeyCode;
    /// use texera::KeyCombination;
    /// use texera::DefaultKeymap;
    /// use texera::EditorAction;
    ///
    /// let json = r#"{
    ///     "ctrl-z": "undo",
    ///     "ctrl-y": "redo"
    /// }"#;
    /// let keymap = DefaultKeymap::from_json_str(json).unwrap();
    /// let combo = KeyCombination::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
    /// assert_eq!(keymap.bindings.get(&combo), Some(&EditorAction::Undo));
    /// ```
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        let mut keymap = Self::default();
        keymap.extend_from_json(json)?;
        Ok(keymap)
    }

    /// Load keybindings from a JSON file in the filesystem.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use texera::DefaultKeymap;
    ///
    /// let keymap = DefaultKeymap::from_file("config/keymap.json");
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_json_str(&content)
    }

    /// Extend bindings from a JSON mapping string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::DefaultKeymap;
    /// use texera::EditorAction;
    ///
    /// let mut keymap = DefaultKeymap::new();
    /// keymap.extend_from_json(r#"{"ctrl-d": "delete_forward"}"#).unwrap();
    /// ```
    pub fn extend_from_json(&mut self, json: &str) -> Result<(), String> {
        let raw: HashMap<String, String> =
            serde_json::from_str(json).map_err(|e| format!("Invalid keymap JSON: {}", e))?;

        for (combo_str, action_str) in raw {
            let combo = KeyCombination::from_str(&combo_str)?;
            let action: EditorAction = serde_json::from_value(serde_json::Value::String(action_str.clone()))
                .map_err(|e| format!("Unknown action '{}': {}", action_str, e))?;
            self.bindings.insert(combo, action);
        }
        Ok(())
    }

    /// Alias for [`DefaultKeymap::extend_from_json`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::DefaultKeymap;
    ///
    /// let mut keymap = DefaultKeymap::new();
    /// keymap.load_from_json(r#"{"ctrl-k": "select_line_end"}"#).unwrap();
    /// ```
    pub fn load_from_json(&mut self, json: &str) -> Result<(), String> {
        self.extend_from_json(json)
    }

    /// Bind a single combination string to an action.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::DefaultKeymap;
    /// use texera::EditorAction;
    ///
    /// let mut keymap = DefaultKeymap::new();
    /// keymap.bind("ctrl-shift-z", EditorAction::Redo).unwrap();
    /// ```
    pub fn bind(&mut self, combo_str: &str, action: EditorAction) -> Result<(), String> {
        let combo = KeyCombination::from_str(combo_str)?;
        self.bindings.insert(combo, action);
        Ok(())
    }
}

impl Keymap for DefaultKeymap {
    fn action_for_key(&self, event: &KeyEvent) -> Option<EditorAction> {
        // Strip non-standard modifiers for lookup
        let clean_mods = event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::SUPER);
        let combo = KeyCombination::new(event.code, clean_mods);

        if let Some(action) = self.bindings.get(&combo).copied() {
            return Some(action);
        }

        // Fallback for typing regular characters
        if clean_mods.is_empty() || clean_mods == KeyModifiers::SHIFT {
            if let KeyCode::Char(_) = event.code {
                return Some(EditorAction::InsertChar);
            }
        }

        None
    }
}
