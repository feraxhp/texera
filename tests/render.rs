use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::widgets::StatefulWidget;
use texera::{EditorView, EditorViewState, LineNumberPlugin};
use texera_core::{AllowMultipleSelections, Selection, State};

#[test]
fn test_render_minimal_without_gutter() {
    let state = State::from("Hello, Texera!");
    let view = EditorView::new(); // Sin gutter por defecto

    let area = Rect::new(0, 0, 20, 5);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Should start at column 0 since there is no gutter
    assert_eq!(buf[(0, 0)].symbol(), "H");
    assert_eq!(buf[(1, 0)].symbol(), "e");
    assert_eq!(buf[(2, 0)].symbol(), "l");
    assert_eq!(buf[(3, 0)].symbol(), "l");
    assert_eq!(buf[(4, 0)].symbol(), "o");
}

#[test]
fn test_render_with_line_gutter() {
    let state = State::from("First\nSecond\nThird");
    let view = EditorView::new().with_gutter(LineNumberPlugin::default());

    let area = Rect::new(0, 0, 25, 5);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Gutter width for 3 lines with padding 1 is 2 ("1 ", "2 ", "3 ")
    // Column 0 should have line number '1'
    assert_eq!(buf[(0, 0)].symbol(), "1");
    // Column 2 should have the first letter 'F' of "First"
    assert_eq!(buf[(2, 0)].symbol(), "F");
    assert_eq!(buf[(0, 1)].symbol(), "2");
    assert_eq!(buf[(2, 1)].symbol(), "S");
}

#[test]
fn test_render_text_selection() {
    // Select "World" in "Hello World" -> from 6 to 11
    let selection = Selection::from([Selection::range(6, 11)]);
    let state = State::with_selection("Hello World", selection);
    let view = EditorView::new();

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // "Hello " (cols 0..6) should NOT have selection background
    assert_ne!(buf[(0, 0)].style().bg, Some(Color::DarkGray));

    // "World" (cols 6..11) MUST have selection background
    for col in 6..11 {
        assert_eq!(
            buf[(col, 0)].style().bg,
            Some(Color::DarkGray),
            "Column {} should have selection background",
            col
        );
    }
}

#[test]
fn test_render_multiple_cursors() {
    let state = State::with_selection(
        "aaa\nbbb\nccc",
        Selection::from([
            Selection::cursor(0), // Secondary cursor at 'a' (row 0, col 0)
            Selection::cursor(4), // Primary cursor at 'b' (row 1, col 0)
        ]),
    )
    .with_extensions(AllowMultipleSelections(true));

    let view = EditorView::new();
    let area = Rect::new(0, 0, 20, 5);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // In CodeMirror/Texera Selection::from, the last element is the primary cursor
    // Primary cursor at (0, 1) has Modifier::REVERSED
    assert!(buf[(0, 1)].style().add_modifier.contains(Modifier::REVERSED));

    // Secondary cursor at (0, 0) has DarkGray background occupying the full character cell (no underline)
    assert_eq!(buf[(0, 0)].style().bg, Some(Color::DarkGray));
    assert!(!buf[(0, 0)].style().add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn test_render_soft_wrapping() {
    let state = State::from("word1 word2 word3 word4");
    // View with wrap enabled and width 12
    let view = EditorView::new().with_wrap(true);

    let area = Rect::new(0, 0, 12, 5);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Row 0 should contain "word1 word2"
    assert_eq!(buf[(0, 0)].symbol(), "w");
    // Row 1 should contain "word3 word4" wrapped softly
    assert_eq!(buf[(0, 1)].symbol(), "w");
}
