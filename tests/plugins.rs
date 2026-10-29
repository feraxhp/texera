use ratatui::widgets::StatefulWidget;
use texera::CustomDecorationPlugin;
use texera::UnsavedLinePlugin;
use texera::LineNumberPlugin;
use ratatui::buffer::Buffer;
use texera::EditorViewState;
use texera::CommandHandler;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use texera::HistoryPlugin;
use texera::EditorView;
use texera_core::State;
use texera_core::tr;

#[test]
fn unsaved_lines_indicator_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("line 1\nline 2\nline 3");
    let mut unsaved = UnsavedLinePlugin::new();

    // Mark as initially clean
    unsaved.mark_as_saved(&state);
    assert!(!unsaved.is_dirty());
    assert!(!unsaved.is_line_dirty(2));

    // Edit line 2: replace "line 2" with "line 2 modified"
    // "line 1\n" is 7 chars. Line 2 starts at 7, ends at 13
    state = state
        .update(tr!({ changes: { from: 7, to: 13, insert: "line 2 modified" } }))?
        .into_state();

    unsaved.check_dirty_lines(&state);
    assert!(unsaved.is_dirty());
    assert!(unsaved.is_line_dirty(2));
    assert!(!unsaved.is_line_dirty(1));
    assert!(!unsaved.is_line_dirty(3));

    // Re-save
    unsaved.mark_as_saved(&state);
    assert!(!unsaved.is_dirty());
    assert!(!unsaved.is_line_dirty(2));

    Ok(())
}

#[test]
fn unsaved_lines_newline_insertion_marks_only_new_line_dirty() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("alpha\nbeta\ngamma");
    let mut unsaved = UnsavedLinePlugin::new();
    unsaved.mark_as_saved(&state);

    // Insert a newline after "alpha\n" (index 6):
    // "alpha\n" is 6 chars. Inserting "\n" produces:
    // line 1: "alpha"
    // line 2: ""
    // line 3: "beta"
    // line 4: "gamma"
    state = state
        .update(tr!({ changes: { from: 6, insert: "\n" } }))?
        .into_state();

    unsaved.check_dirty_lines(&state);
    assert!(unsaved.is_dirty());
    assert!(!unsaved.is_line_dirty(1), "line 1 should be clean");
    assert!(unsaved.is_line_dirty(2), "new line 2 should be dirty");
    assert!(!unsaved.is_line_dirty(3), "subsequent line 3 ('beta') should NOT be dirty");
    assert!(!unsaved.is_line_dirty(4), "subsequent line 4 ('gamma') should NOT be dirty");

    // Insert another line at the end of the document
    let doc_len = state.doc().len_chars();
    state = state
        .update(tr!({ changes: { from: doc_len, insert: "\nnew tail" } }))?
        .into_state();

    unsaved.check_dirty_lines(&state);
    assert!(!unsaved.is_line_dirty(1), "line 1 should be clean");
    assert!(unsaved.is_line_dirty(2), "line 2 should be dirty");
    assert!(!unsaved.is_line_dirty(3), "line 3 ('beta') should be clean");
    assert!(!unsaved.is_line_dirty(4), "line 4 ('gamma') should be clean");
    assert!(unsaved.is_line_dirty(5), "line 5 ('new tail') should be dirty");

    Ok(())
}

#[test]
fn render_unsaved_lines_in_yellow_beside_gutter() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("alpha\nbeta\ngamma");
    let mut unsaved = UnsavedLinePlugin::new();
    unsaved.mark_as_saved(&state);

    // Edit line 2 ("beta")
    state = state
        .update(tr!({ changes: { from: 6, to: 10, insert: "beta_edit" } }))?
        .into_state();
    unsaved.check_dirty_lines(&state);

    // Gutter with unsaved marker attached
    let gutter = LineNumberPlugin::new().with_marker(unsaved);
    let view = EditorView::new().with_gutter(gutter);

    let area = Rect::new(0, 0, 30, 5);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Row 0 (line 1: clean) -> marker at col 0 is space, number at col 1
    assert_eq!(buf[(0, 0)].symbol(), " ");
    assert_eq!(buf[(1, 0)].symbol(), "1");

    // Row 1 (line 2: dirty) -> marker at col 0 MUST have yellow indicator "┃" BEFORE number at col 1
    assert_eq!(buf[(0, 1)].symbol(), "┃");
    assert_eq!(buf[(0, 1)].style().fg, Some(Color::Yellow));
    assert_eq!(buf[(1, 1)].symbol(), "2");

    // Row 2 (line 3: clean)
    assert_eq!(buf[(0, 2)].symbol(), " ");
    assert_eq!(buf[(1, 2)].symbol(), "3");

    Ok(())
}

#[test]
fn history_plugin_undo_and_redo() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("hello")
        .with_extensions(HistoryPlugin::extension(None));

    assert_eq!(HistoryPlugin::undo_depth(&state), 0);

    // Edit: "hello" -> "hello world"
    state = state
        .update(tr!({ changes: { from: 5, insert: " world" } }))?
        .into_state();
    assert_eq!(state.doc().to_string(), "hello world");
    assert_eq!(HistoryPlugin::undo_depth(&state), 1);
    assert_eq!(HistoryPlugin::redo_depth(&state), 0);

    // Undo via HistoryPlugin
    let undid = HistoryPlugin::undo(&mut state);
    assert!(undid);
    assert_eq!(state.doc().to_string(), "hello");
    assert_eq!(HistoryPlugin::undo_depth(&state), 0);
    assert_eq!(HistoryPlugin::redo_depth(&state), 1);

    // Redo via HistoryPlugin
    let redid = HistoryPlugin::redo(&mut state);
    assert!(redid);
    assert_eq!(state.doc().to_string(), "hello world");
    assert_eq!(HistoryPlugin::undo_depth(&state), 1);
    assert_eq!(HistoryPlugin::redo_depth(&state), 0);

    Ok(())
}

#[test]
fn history_commander_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::from("abc")
        .with_extensions(HistoryPlugin::extension(None));

    let plugin = HistoryPlugin::new();
    let mut commander = plugin.commander(&mut state);

    // Dispatch transaction
    commander
        .dispatch_transaction(tr!({ changes: { from: 3, insert: "def" } }))?;

    assert_eq!(commander.state.doc().to_string(), "abcdef");

    // Undo via commander
    assert!(commander.undo());
    assert_eq!(commander.state.doc().to_string(), "abc");

    // Redo via commander
    assert!(commander.redo());
    assert_eq!(commander.state.doc().to_string(), "abcdef");

    Ok(())
}

#[test]
fn custom_decoration_plugin_render() {
    let state = State::from("critical warning text");

    // Decorate "warning" (from 9 to 16) with Yellow foreground
    let dec = CustomDecorationPlugin::empty()
        .with_mark(9..16, Style::default().fg(Color::Yellow));

    let view = EditorView::new().with_decoration_provider(dec);

    let area = Rect::new(0, 0, 30, 3);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // "critical " (0..9) should not be Yellow
    assert_ne!(buf[(0, 0)].style().fg, Some(Color::Yellow));

    // "warning" (cols 9..16) must be Yellow
    for col in 9..16 {
        assert_eq!(
            buf[(col, 0)].style().fg,
            Some(Color::Yellow),
            "Column {} should have yellow foreground",
            col
        );
    }
}

#[cfg(feature = "whitespace")]
#[test]
fn whitespace_plugin_default_and_custom() {
    use texera::WhitespacePlugin;

    // 1. Verify Default settings
    let plugin = WhitespacePlugin::default();
    assert_eq!(plugin.space_char, '·');
    assert_eq!(plugin.tab_filler, ' ');
    assert_eq!(plugin.tab_terminator, '⇥');
    assert_eq!(plugin.style, None);

    let state = State::from(" \t");
    let view = EditorView::new().with_glyph_transformer(plugin);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Space rendered as '·'
    assert_eq!(buf[(0, 0)].symbol(), "·");
    // Tab rendered with filler ' ' and terminator '⇥'
    assert_eq!(buf[(1, 0)].symbol(), " ");
    assert_eq!(buf[(2, 0)].symbol(), " ");
    assert_eq!(buf[(3, 0)].symbol(), "⇥");

    // 2. Verify Custom builder methods
    let custom_plugin = WhitespacePlugin::new()
        .with_space_char('•')
        .with_tab_filler('─')
        .with_tab_terminator('>')
        .with_style(Style::default().fg(Color::Magenta));

    assert_eq!(custom_plugin.space_char, '•');
    assert_eq!(custom_plugin.tab_filler, '─');
    assert_eq!(custom_plugin.tab_terminator, '>');
    assert_eq!(custom_plugin.style, Some(Style::default().fg(Color::Magenta)));

    let mut buf_custom = Buffer::empty(area);
    let mut view_state_custom = EditorViewState::new();
    let view_custom = EditorView::new().with_glyph_transformer(custom_plugin);

    view_custom.bind(&state).render(area, &mut buf_custom, &mut view_state_custom);

    assert_eq!(buf_custom[(0, 0)].symbol(), "•");
    assert_eq!(buf_custom[(0, 0)].style().fg, Some(Color::Magenta));
    assert_eq!(buf_custom[(1, 0)].symbol(), "─");
    assert_eq!(buf_custom[(2, 0)].symbol(), "─");
    assert_eq!(buf_custom[(3, 0)].symbol(), ">");
    assert_eq!(buf_custom[(3, 0)].style().fg, Some(Color::Magenta));
}
