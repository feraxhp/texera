use std::str::FromStr;

use texera_core::AllowMultipleSelections;
use crossterm::event::KeyModifiers;
use crossterm::event::KeyEvent;
use crossterm::event::KeyCode;
use texera::KeyCombination;
use texera_core::Selection;
use texera::DefaultKeymap;
use texera::HistoryPlugin;
use texera::EditorAction;
use texera::apply_action;
use texera_core::State;
use texera_core::tr;
use texera::Keymap;

#[test]
fn key_combination_from_str() -> Result<(), Box<dyn std::error::Error>> {
    let k1 = KeyCombination::from_str("ctrl-z")?;
    assert_eq!(k1.code, KeyCode::Char('z'));
    assert_eq!(k1.modifiers, KeyModifiers::CONTROL);

    let k2 = KeyCombination::from_str("ctrl-shift-z")?;
    assert_eq!(k2.code, KeyCode::Char('z'));
    assert_eq!(k2.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);

    let k3 = KeyCombination::from_str("alt-arrow_down")?;
    assert_eq!(k3.code, KeyCode::Down);
    assert_eq!(k3.modifiers, KeyModifiers::ALT);

    let k4 = KeyCombination::from_str("backspace")?;
    assert_eq!(k4.code, KeyCode::Backspace);
    assert!(k4.modifiers.is_empty());

    Ok(())
}

#[test]
fn default_keymap_resolution() {
    let keymap = DefaultKeymap::new();

    let ev_ctrl_z = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(keymap.action_for_key(&ev_ctrl_z), Some(EditorAction::Undo));

    let ev_ctrl_y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(keymap.action_for_key(&ev_ctrl_y), Some(EditorAction::Redo));

    let ev_bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
    assert_eq!(keymap.action_for_key(&ev_bs), Some(EditorAction::DeleteBackward));

    let ev_char = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
    assert_eq!(keymap.action_for_key(&ev_char), Some(EditorAction::InsertChar));
}

#[test]
fn keymap_load_from_json() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{"ctrl-s": "select_all", "alt-x": "delete_forward", "ctrl-q": "undo"}"#;

    let keymap = DefaultKeymap::from_json_str(json)?;

    let ev_ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(keymap.action_for_key(&ev_ctrl_s), Some(EditorAction::SelectAll));

    let ev_alt_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(keymap.action_for_key(&ev_alt_x), Some(EditorAction::DeleteForward));

    Ok(())
}

#[test]
fn apply_action_typing_and_navigation() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("");

    // 1. Type "h", "i"
    let ev_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty());
    apply_action(&mut state, EditorAction::InsertChar, Some(&ev_h))?;
    let ev_i = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::empty());
    apply_action(&mut state, EditorAction::InsertChar, Some(&ev_i))?;
    assert_eq!(state.doc().to_string(), "hi");
    assert_eq!(state.selection().main().head(), 2);

    // 2. Move left
    apply_action(&mut state, EditorAction::MoveLeft, None)?;
    assert_eq!(state.selection().main().head(), 1);

    // 3. Backspace at offset 1 -> deletes 'h', leaving 'i'
    apply_action(&mut state, EditorAction::DeleteBackward, None)?;
    assert_eq!(state.doc().to_string(), "i");
    assert_eq!(state.selection().main().head(), 0);

    // 4. Enter -> new line
    apply_action(&mut state, EditorAction::InsertNewline, None)?;
    assert_eq!(state.doc().to_string(), "\ni");

    Ok(())
}

#[test]
fn apply_action_undo_redo_and_multicursor() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("line1\nline2")
        .with_extensions([HistoryPlugin::extension(None), AllowMultipleSelections(true).into()]);

    // Position at offset 0
    // Alt-Down -> Add cursor below at offset 6 (start of line2)
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);

    // Type 'X' at both cursor positions
    let ev_x = KeyEvent::new(KeyCode::Char('X'), KeyModifiers::empty());
    apply_action(&mut state, EditorAction::InsertChar, Some(&ev_x))?;
    assert_eq!(state.doc().to_string(), "Xline1\nXline2");
    assert_eq!(state.selection().ranges_ref()[0].head(), 1);
    assert_eq!(state.selection().ranges_ref()[1].head(), 8);

    // Undo via action
    apply_action(&mut state, EditorAction::Undo, None)?;
    assert_eq!(state.doc().to_string(), "line1\nline2");

    // Redo via action
    apply_action(&mut state, EditorAction::Redo, None)?;
    assert_eq!(state.doc().to_string(), "Xline1\nXline2");
    assert_eq!(state.selection().ranges_ref()[0].head(), 1);
    assert_eq!(state.selection().ranges_ref()[1].head(), 8);

    // Delete backward with multiple cursors:
    // Cursors are at 1 and 8. DeleteBackward removes 'X' from both lines.
    apply_action(&mut state, EditorAction::DeleteBackward, None)?;
    assert_eq!(state.doc().to_string(), "line1\nline2");
    // Verify cursors are placed at EXACT correct positions:
    // Cursor 0 at start of line 1 (0), Cursor 1 at start of line 2 (6)
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().ranges_ref()[0].head(), 0);
    assert_eq!(state.selection().ranges_ref()[1].head(), 6);

    // Delete forward with multiple cursors:
    // Cursors are at 0 (before 'l') and 6 (before 'l'). DeleteForward deletes 'l' from both lines.
    apply_action(&mut state, EditorAction::DeleteForward, None)?;
    assert_eq!(state.doc().to_string(), "ine1\nine2");
    // Cursors must remain at the exact deletion positions:
    // Cursor 0 at offset 0 (before 'i'), Cursor 1 at offset 5 (before 'i' in "ine2")
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().ranges_ref()[0].head(), 0);
    assert_eq!(state.selection().ranges_ref()[1].head(), 5);

    Ok(())
}

#[test]
fn bidirectional_cursor_add_and_contract() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("line1\nline2\nline3\nline4")
        .with_extensions([AllowMultipleSelections(true).into()]);

    // Start at line 2 (offset 6)
    state = state.update(tr!({
        selection: Selection::cursor(6),
    }))?.into_state();
    assert_eq!(state.selection().ranges_ref().len(), 1);

    // 1. Add cursor below -> lines 2, 3
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().main_index(), 1);

    // Add cursor below -> lines 2, 3, 4
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 3);
    assert_eq!(state.selection().main_index(), 2);

    // 2. We were expanding downwards, now press AddCursorAbove -> contracts the bottommost cursor!
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().main_index(), 1);

    // Press AddCursorAbove again -> contracts to line 2 (only 1 cursor left)
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 1);
    assert_eq!(state.selection().ranges_ref()[0].head(), 6);

    // 3. Now only 1 cursor left, press AddCursorAbove -> expands upwards! (lines 1, 2)
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().main_index(), 0);

    // 4. We were expanding upwards, now press AddCursorBelow -> contracts the topmost cursor!
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 1);
    assert_eq!(state.selection().ranges_ref()[0].head(), 6);

    Ok(())
}

#[test]
fn multicursor_upward_movement_and_top_line_edge_case() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("line1\nline2\nline3\nline4")
        .with_extensions([AllowMultipleSelections(true).into()]);

    // Test edge case 1: Cursor at line 0 (top line, no line above)
    assert_eq!(state.selection().main().head(), 0);
    assert_eq!(state.selection().main_index(), 0);
    // Attempting to add cursor above line 0
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    // Should stay at 1 cursor, head 0, main_index 0 without panicking or losing track
    assert_eq!(state.selection().ranges_ref().len(), 1);
    assert_eq!(state.selection().main_index(), 0);
    assert_eq!(state.selection().main().head(), 0);

    // Test edge case 2: Adding cursors upwards and then moving cursors
    // Start cursor at line 3 (offset 12)
    state = state.update(tr!({
        selection: Selection::cursor(12),
    }))?.into_state();
    assert_eq!(state.selection().main().head(), 12);

    // Add cursor above -> adds at line 2 (offset 6)
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().main_index(), 0); // main cursor is line 2 (topmost)

    // Move cursors right and left
    apply_action(&mut state, EditorAction::MoveRight, None)?;
    assert_eq!(state.selection().main_index(), 0); // MUST PRESERVE main_index = 0!
    assert_eq!(state.selection().ranges_ref()[0].head(), 7);
    assert_eq!(state.selection().ranges_ref()[1].head(), 13);

    apply_action(&mut state, EditorAction::MoveLeft, None)?;
    assert_eq!(state.selection().main_index(), 0); // MUST PRESERVE main_index = 0!
    assert_eq!(state.selection().ranges_ref()[0].head(), 6);
    assert_eq!(state.selection().ranges_ref()[1].head(), 12);

    // Since we were adding upwards and preserved main_index == 0, calling AddCursorAbove
    // MUST add another cursor upwards (at line 1 / line 0-index), NOT delete a cursor!
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 3);
    assert_eq!(state.selection().main_index(), 0);
    assert_eq!(state.selection().ranges_ref()[0].head(), 0); // line 1
    assert_eq!(state.selection().ranges_ref()[1].head(), 6); // line 2
    assert_eq!(state.selection().ranges_ref()[2].head(), 12); // line 3

    // Now the topmost cursor is at line 0 (inexistent line above):
    // Attempting to AddCursorAbove does not delete or break state, main_index remains 0
    apply_action(&mut state, EditorAction::AddCursorAbove, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 3);
    assert_eq!(state.selection().main_index(), 0);

    // Now calling AddCursorBelow contracts the topmost cursor (downwards reversal)
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 2);
    assert_eq!(state.selection().main_index(), 0);
    assert_eq!(state.selection().ranges_ref()[0].head(), 6);
    assert_eq!(state.selection().ranges_ref()[1].head(), 12);

    // Another MoveLineEnd preserves main_index = 0
    apply_action(&mut state, EditorAction::MoveLineEnd, None)?;
    assert_eq!(state.selection().main_index(), 0);

    // Another AddCursorBelow contracts down to single cursor
    apply_action(&mut state, EditorAction::AddCursorBelow, None)?;
    assert_eq!(state.selection().ranges_ref().len(), 1);
    assert_eq!(state.selection().main().head(), 17); // line 3 end

    Ok(())
}
