// Copyright (C) 2026 feraxhp
// 
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.\n// 
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
// 
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Keymap handling and action dispatching.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use texera_core::{tr, Selection, SelectionRange, State};
use texera_pluggins::{redo_state, undo_state};

use crate::input::action::{EditorAction, KeyCombination};

/// Trait resolving terminal [`KeyEvent`]s to [`EditorAction`]s.
///
/// Implement this trait to provide custom keybinding logic or custom keymap profiles.
///
/// # Examples
///
/// ```rust
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use texera::{EditorAction, Keymap};
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
    ///
    /// Returns `Some(action)` if a binding matches the event, or `None` otherwise.
    fn action_for_key(&self, event: &KeyEvent) -> Option<EditorAction>;
}

/// The default terminal keymap relating key combinations to editor actions.
/// Can be customized dynamically or loaded from JSON files.
///
/// # Examples
///
/// ```rust
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, KeyEventKind, KeyEventState};
/// use texera::{DefaultKeymap, EditorAction, Keymap};
///
/// let mut keymap = DefaultKeymap::new();
/// keymap.bind("ctrl-s", EditorAction::SelectAll).unwrap();
///
/// let event = KeyEvent {
///     code: KeyCode::Char('s'),
///     modifiers: KeyModifiers::CONTROL,
///     kind: KeyEventKind::Press,
///     state: KeyEventState::empty(),
/// };
/// assert_eq!(keymap.action_for_key(&event), Some(EditorAction::SelectAll));
/// ```
#[derive(Clone, Debug)]
pub struct DefaultKeymap {
    /// Map of key combinations to editor actions.
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

        Self { bindings: map }
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
    pub fn new() -> Self {
        Self::default()
    }

    /// Load or extend keybindings from a JSON string mapping `"key-combo": "action_name"`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{DefaultKeymap, EditorAction, KeyCombination};
    /// use crossterm::event::{KeyCode, KeyModifiers};
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
        let content = fs::read_to_string(path).map_err(|e| e.to_string())? ;
        Self::from_json_str(&content)
    }

    /// Extend bindings from a JSON mapping string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{DefaultKeymap, EditorAction};
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
    /// use texera::{DefaultKeymap, EditorAction};
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

/// Applies an [`EditorAction`] directly onto the given mutable [`State`].
///
/// Returns `Ok(true)` if the action mutated the document text or selection, or `Ok(false)`
/// if no change was made.
///
/// # Examples
///
/// ```rust
/// use texera::{apply_action, EditorAction};
/// use texera_core::State;
///
/// let mut state = State::from("Hello world");
/// let modified = apply_action(&mut state, EditorAction::SelectAll, None).unwrap();
/// assert!(modified);
/// ```
pub fn apply_action(
    state: &mut State,
    action: EditorAction,
    event: Option<&KeyEvent>,
) -> Result<bool, mynox::Error> {
    match action {
        EditorAction::Undo => Ok(undo_state(state)),
        EditorAction::Redo => Ok(redo_state(state)),
        EditorAction::InsertChar => {
            if let Some(ev) = event {
                if let KeyCode::Char(c) = ev.code {
                    let spec = state.replace_selection(&c.to_string());
                    let next = state.clone().update(spec)?.into_state();
                    *state = next;
                    return Ok(true);
                }
            }
            Ok(false)
        }
        EditorAction::InsertNewline => {
            let spec = state.replace_selection("\n");
            let next = state.clone().update(spec)?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::InsertTab => {
            let spec = state.replace_selection("    ");
            let next = state.clone().update(spec)?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::DeleteBackward => {
            let can_delete = state.selection().ranges_ref().iter().any(|r| !r.empty() || r.head() > 0);
            if !can_delete {
                return Ok(false);
            }

            let spec = state.change_by_range(|(range, _is_main)| {
                if !range.empty() {
                    let from = range.from();
                    let to = range.to();
                    Ok(tr!({
                        changes: { from: from, to: to, insert: "" },
                        selection: Selection::cursor(from),
                    }))
                } else if range.head() > 0 {
                    let from = range.head() - 1;
                    let to = range.head();
                    Ok(tr!({
                        changes: { from: from, to: to, insert: "" },
                        selection: Selection::cursor(from),
                    }))
                } else {
                    Ok(tr!({
                        selection: Selection::cursor(0),
                    }))
                }
            })?;

            let next = state.clone().update(spec.with_user_event("delete.backward"))?.into_state();
            let changed = *next.doc() != *state.doc() || *next.selection() != *state.selection();
            *state = next;
            Ok(changed)
        }
        EditorAction::DeleteForward => {
            let doc_len = state.doc().len_chars();
            let can_delete = state.selection().ranges_ref().iter().any(|r| !r.empty() || r.head() < doc_len);
            if !can_delete {
                return Ok(false);
            }

            let spec = state.change_by_range(|(range, _is_main)| {
                if !range.empty() {
                    let from = range.from();
                    let to = range.to();
                    Ok(tr!({
                        changes: { from: from, to: to, insert: "" },
                        selection: Selection::cursor(from),
                    }))
                } else if range.head() < doc_len {
                    let from = range.head();
                    let to = range.head() + 1;
                    Ok(tr!({
                        changes: { from: from, to: to, insert: "" },
                        selection: Selection::cursor(from),
                    }))
                } else {
                    Ok(tr!({
                        selection: Selection::cursor(range.head()),
                    }))
                }
            })?;

            let next = state.clone().update(spec.with_user_event("delete.forward"))?.into_state();
            let changed = *next.doc() != *state.doc() || *next.selection() != *state.selection();
            *state = next;
            Ok(changed)
        }
        EditorAction::MoveLeft => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                let target = range.head().saturating_sub(1);
                new_ranges.push(Selection::cursor(target));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::MoveRight => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            let doc_len = state.doc().len_chars();
            for range in state.selection().ranges_ref() {
                let target = (range.head() + 1).min(doc_len);
                new_ranges.push(Selection::cursor(target));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectLeft => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                let new_head = range.head().saturating_sub(1);
                new_ranges.push(Selection::range(range.anchor(), new_head));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectRight => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            let doc_len = state.doc().len_chars();
            for range in state.selection().ranges_ref() {
                let new_head = (range.head() + 1).min(doc_len);
                new_ranges.push(Selection::range(range.anchor(), new_head));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::MoveUp => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    let col = range.head().saturating_sub(line.from());
                    if line.number() > 0 {
                        if let Ok(prev_line) = state.doc().line(line.number() - 1) {
                            let target = prev_line.from() + col.min(prev_line.length_trimmed());
                            new_ranges.push(Selection::cursor(target));
                            continue;
                        }
                    }
                }
                new_ranges.push(Selection::cursor(range.head()));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::MoveDown => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            let total_lines = state.doc().len_lines();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    let col = range.head().saturating_sub(line.from());
                    if line.number() + 1 < total_lines {
                        if let Ok(next_line) = state.doc().line(line.number() + 1) {
                            let target = next_line.from() + col.min(next_line.length_trimmed());
                            new_ranges.push(Selection::cursor(target));
                            continue;
                        }
                    }
                }
                new_ranges.push(Selection::cursor(range.head()));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::MoveLineStart => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    new_ranges.push(Selection::cursor(line.from()));
                } else {
                    new_ranges.push(Selection::cursor(range.head()));
                }
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::MoveLineEnd => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    new_ranges.push(Selection::cursor(line.from() + line.length_trimmed()));
                } else {
                    new_ranges.push(Selection::cursor(range.head()));
                }
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectLineStart => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    new_ranges.push(Selection::range(range.anchor(), line.from()));
                } else {
                    new_ranges.push(Selection::range(range.anchor(), range.head()));
                }
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectLineEnd => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    new_ranges.push(Selection::range(range.anchor(), line.from() + line.length_trimmed()));
                } else {
                    new_ranges.push(Selection::range(range.anchor(), range.head()));
                }
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectUp => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    let col = range.head().saturating_sub(line.from());
                    if line.number() > 0 {
                        if let Ok(prev_line) = state.doc().line(line.number() - 1) {
                            let target = prev_line.from() + col.min(prev_line.length_trimmed());
                            new_ranges.push(Selection::range(range.anchor(), target));
                            continue;
                        }
                    }
                }
                new_ranges.push(Selection::range(range.anchor(), range.head()));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::SelectDown => {
            let current_main = state.selection().main_index();
            let mut new_ranges = Vec::new();
            let total_lines = state.doc().len_lines();
            for range in state.selection().ranges_ref() {
                if let Ok(line) = state.doc().line_at(range.head()) {
                    let col = range.head().saturating_sub(line.from());
                    if line.number() + 1 < total_lines {
                        if let Ok(next_line) = state.doc().line(line.number() + 1) {
                            let target = next_line.from() + col.min(next_line.length_trimmed());
                            new_ranges.push(Selection::range(range.anchor(), target));
                            continue;
                        }
                    }
                }
                new_ranges.push(Selection::range(range.anchor(), range.head()));
            }
            let sel = Selection::from_with_head(new_ranges, current_main.min(state.selection().ranges_ref().len().saturating_sub(1)))?;
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true }))?.into_state();
            *state = next;
            Ok(true)
        }
        EditorAction::AddCursorBelow => {
            let mut ranges: Vec<SelectionRange> = state.selection().ranges_ref().to_vec();
            let current_main = state.selection().main_index();

            // 1. If we previously expanded upwards (main is at index 0) and have multiple cursors,
            // pressing Down contracts/removes the topmost added cursor.
            if ranges.len() > 1 && current_main == 0 {
                ranges.remove(0);
                let sel = Selection::from_with_head(ranges, 0)?;
                let next = state.clone().update(tr!({
                    selection: sel,
                    scroll_into_view: true,
                }))?.into_state();
                *state = next;
                return Ok(true);
            }

            // 2. Otherwise, expand downwards from the bottommost cursor
            if let Some(bottom) = ranges.last().cloned() {
                if let Ok(line) = state.doc().line_at(bottom.head()) {
                    let col = bottom.head().saturating_sub(line.from());
                    if line.number() + 1 < state.doc().len_lines() {
                        if let Ok(next_line) = state.doc().line(line.number() + 1) {
                            let target = next_line.from() + col.min(next_line.length_trimmed());
                            ranges.push(Selection::cursor(target));
                            let new_main = ranges.len() - 1;
                            let sel = Selection::from_with_head(ranges, new_main)?;
                            let next = state.clone().update(tr!({
                                selection: sel,
                                scroll_into_view: true,
                            }))?.into_state();
                            *state = next;
                            return Ok(true);
                        }
                    } else {
                        // Already at last line (cannot add below).
                        // Keep main_index firmly at bottom so active cursor stays at the bottom
                        // and expanding downwards direction is strictly maintained.
                        let new_main = ranges.len() - 1;
                        let sel = Selection::from_with_head(ranges, new_main)?;
                        let next = state.clone().update(tr!({
                            selection: sel,
                            scroll_into_view: true,
                        }))?.into_state();
                        *state = next;
                        return Ok(false);
                    }
                }
            }
            Ok(false)
        }
        EditorAction::AddCursorAbove => {
            let mut ranges: Vec<SelectionRange> = state.selection().ranges_ref().to_vec();
            let current_main = state.selection().main_index();

            // 1. If we previously expanded downwards (main is at the bottom) and have multiple cursors,
            // pressing Up contracts/removes the bottommost added cursor.
            if ranges.len() > 1 && current_main == ranges.len() - 1 {
                ranges.pop();
                let new_main = ranges.len() - 1;
                let sel = Selection::from_with_head(ranges, new_main)?;
                let next = state.clone().update(tr!({
                    selection: sel,
                    scroll_into_view: true,
                }))?.into_state();
                *state = next;
                return Ok(true);
            }

            // 2. Otherwise, expand upwards from the topmost cursor
            if let Some(top) = ranges.first().cloned() {
                if let Ok(line) = state.doc().line_at(top.head()) {
                    let col = top.head().saturating_sub(line.from());
                    if line.number() > 0 {
                        if let Ok(prev_line) = state.doc().line(line.number() - 1) {
                            let target = prev_line.from() + col.min(prev_line.length_trimmed());
                            ranges.insert(0, Selection::cursor(target));
                            let sel = Selection::from_with_head(ranges, 0)?;
                            let next = state.clone().update(tr!({
                                selection: sel,
                                scroll_into_view: true,
                            }))?.into_state();
                            *state = next;
                            return Ok(true);
                        }
                    } else {
                        // Already at line 0 (cannot add above).
                        // Keep main_index firmly at 0 so active cursor stays at the top
                        // and expanding upwards direction is strictly maintained.
                        let sel = Selection::from_with_head(ranges, 0)?;
                        let next = state.clone().update(tr!({
                            selection: sel,
                            scroll_into_view: true,
                        }))?.into_state();
                        *state = next;
                        return Ok(false);
                    }
                }
            }
            Ok(false)
        }
        EditorAction::SelectAll => {
            let next = state.clone().update(tr!({
                selection: Selection::from([Selection::range(0, state.doc().len_chars())])
            }))?.into_state();
            *state = next;
            Ok(true)
        }
        _ => Ok(false),
    }
}
