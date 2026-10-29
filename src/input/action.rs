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

//! High-level editor actions and key combinations.

use std::str::FromStr;
use std::fmt;

use crossterm::event::KeyModifiers;
use crossterm::event::KeyCode;
use serde::Deserialize;
use serde::Serialize;

/// High-level editor actions mapped from keybindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorAction {
    /// Inserts the typed character into the document at the current selection(s).
    InsertChar,
    /// Inserts a newline character (`\n`) and advances the cursor to the next line.
    InsertNewline,
    /// Inserts a tab (or equivalent spaces) at the current cursor position.
    InsertTab,
    /// Deletes the character or selection immediately preceding the cursor (Backspace).
    DeleteBackward,
    /// Deletes the character or selection immediately following the cursor (Delete).
    DeleteForward,
    /// Moves the cursor one position to the left.
    MoveLeft,
    /// Moves the cursor one position to the right.
    MoveRight,
    /// Moves the cursor up one line.
    MoveUp,
    /// Moves the cursor down one line.
    MoveDown,
    /// Extends or contracts the active selection one character to the left.
    SelectLeft,
    /// Extends or contracts the active selection one character to the right.
    SelectRight,
    /// Extends or contracts the active selection one line up.
    SelectUp,
    /// Extends or contracts the active selection one line down.
    SelectDown,
    /// Moves the cursor to the start of the current logical line.
    MoveLineStart,
    /// Moves the cursor to the end of the current logical line.
    MoveLineEnd,
    /// Extends the selection to the start of the current logical line.
    SelectLineStart,
    /// Extends the selection to the end of the current logical line.
    SelectLineEnd,
    /// Moves the cursor to the very beginning of the document.
    MoveDocStart,
    /// Moves the cursor to the very end of the document.
    MoveDocEnd,
    /// Selects the entire document text.
    SelectAll,
    /// Reverts the last editing transaction (Undo).
    Undo,
    /// Reapplies the previously undone transaction (Redo).
    Redo,
    /// Adds an additional cursor on the line immediately above.
    AddCursorAbove,
    /// Adds an additional cursor on the line immediately below.
    AddCursorBelow,
}

/// Represents a key and modifier combination (e.g. Ctrl+Z, Shift+ArrowLeft).
///
/// # Examples
///
/// ```rust
/// use std::str::FromStr;
///
/// use crossterm::event::KeyModifiers;
/// use crossterm::event::KeyCode;
/// use texera::KeyCombination;
///
/// // Create directly
/// let combo = KeyCombination::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
/// assert_eq!(combo.to_string(), "Control-s");
///
/// // Parse from string
/// let parsed = KeyCombination::from_str("ctrl-s").unwrap();
/// assert_eq!(parsed.code, KeyCode::Char('s'));
/// assert!(parsed.modifiers.contains(KeyModifiers::CONTROL));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombination {
    /// The keyboard key code (e.g. `KeyCode::Char('c')`, `KeyCode::Enter`).
    pub code: KeyCode,
    /// The keyboard modifiers applied (e.g. `KeyModifiers::CONTROL`, `KeyModifiers::ALT`).
    pub modifiers: KeyModifiers,
}

impl KeyCombination {
    /// Creates a new [`KeyCombination`] from the specified key code and modifier bitflags.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crossterm::event::KeyModifiers;
    /// use crossterm::event::KeyCode;
    /// use texera::KeyCombination;
    ///
    /// let combo = KeyCombination::new(KeyCode::Enter, KeyModifiers::CONTROL);
    /// assert_eq!(combo.code, KeyCode::Enter);
    /// assert_eq!(combo.modifiers, KeyModifiers::CONTROL);
    /// ```
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

impl FromStr for KeyCombination {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = KeyModifiers::empty();
        let parts: Vec<&str> = s.split('-').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return Err("Empty key combination".to_string());
        }

        let key_str = parts.last().unwrap();
        for mod_str in &parts[..parts.len() - 1] {
            match mod_str.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
                "alt" | "opt" | "option" => modifiers.insert(KeyModifiers::ALT),
                "shift" => modifiers.insert(KeyModifiers::SHIFT),
                "super" | "win" | "cmd" => modifiers.insert(KeyModifiers::SUPER),
                other => return Err(format!("Unknown modifier: '{}'", other)),
            }
        }

        let code = match key_str.to_ascii_lowercase().as_str() {
            "backspace" => KeyCode::Backspace,
            "enter" | "return" => KeyCode::Enter,
            "left" | "arrow_left" | "arrowleft" => KeyCode::Left,
            "right" | "arrow_right" | "arrowright" => KeyCode::Right,
            "up" | "arrow_up" | "arrowup" => KeyCode::Up,
            "down" | "arrow_down" | "arrowdown" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "page_up" => KeyCode::PageUp,
            "pagedown" | "page_down" => KeyCode::PageDown,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "esc" | "escape" => KeyCode::Esc,
            ch if ch.chars().count() == 1 => {
                let c = ch.chars().next().unwrap();
                KeyCode::Char(c)
            }
            other => return Err(format!("Unknown key code: '{}'", other)),
        };

        Ok(KeyCombination { code, modifiers })
    }
}

impl fmt::Display for KeyCombination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.modifiers.fmt(f)?;
        _ = write!(f, "-")?;
        self.code.fmt(f)
    }
}
