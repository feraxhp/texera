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

//! Action dispatcher applying high-level editor actions onto document state.

use texera_core::SelectionRange;
use texera_pluggins::redo_state;
use texera_pluggins::undo_state;
use crossterm::event::KeyEvent;
use crossterm::event::KeyCode;
use texera_core::Selection;
use texera_core::State;
use texera_core::tr;

use crate::input::action::EditorAction;

/// Applies an [`EditorAction`] directly onto the given mutable [`State`].
///
/// Returns `Ok(true)` if the action mutated the document text or selection, or `Ok(false)`
/// if no change was made.
///
/// # Examples
///
/// ```rust
/// use texera::DefaultKeymap;
/// use texera::EditorAction;
/// use texera::apply_action;
///
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
            let next = state.clone().update(tr!({ selection: sel, scroll_into_view: true, }))?.into_state();
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
                selection: Selection::from([Selection::range(0, state.doc().len_chars())]),
            }))?.into_state();
            *state = next;
            Ok(true)
        }
        _ => Ok(false),
    }
}
