use texera::{
    compute_visual_lines, doc_offset_to_visual_pos, visual_pos_to_doc_offset, CellMetrics,
    Scroller, VisualPosition,
};
use texera_core::Text;

#[test]
fn test_cell_metrics_ascii_and_wide_chars() {
    // Normal ASCII
    assert_eq!(CellMetrics::char_width('a', 0, 4), 1);
    assert_eq!(CellMetrics::string_width("hello", 0, 4), 5);

    // CJK wide character (width 2)
    assert_eq!(CellMetrics::char_width('你', 0, 4), 2);
    assert_eq!(CellMetrics::string_width("你好", 0, 4), 4);

    // Tabs with tab_size = 4
    assert_eq!(CellMetrics::char_width('\t', 0, 4), 4);
    assert_eq!(CellMetrics::char_width('\t', 1, 4), 3);
    assert_eq!(CellMetrics::char_width('\t', 3, 4), 1);
    assert_eq!(CellMetrics::char_width('\t', 4, 4), 4);
}

#[test]
fn test_visual_lines_without_wrap() {
    let doc = Text::from("line one\nline two is longer\n3");
    let visual = compute_visual_lines(&doc, false, 80, 4);

    assert_eq!(visual.len(), 3);
    assert_eq!(visual[0].text, "line one");
    assert!(!visual[0].is_wrapped_continuation);
    assert_eq!(visual[1].text, "line two is longer");
    assert_eq!(visual[2].text, "3");
}

#[test]
fn test_visual_lines_with_soft_wrap() {
    let doc = Text::from("the quick brown fox jumps over the lazy dog");
    // Wrap at 15 columns
    let visual = compute_visual_lines(&doc, true, 15, 4);

    assert!(visual.len() > 1);
    assert_eq!(visual[0].logical_line_index, 0);
    assert!(!visual[0].is_wrapped_continuation);

    // Subsequent segments must be marked as continuation lines
    for v in &visual[1..] {
        assert_eq!(v.logical_line_index, 0);
        assert!(v.is_wrapped_continuation);
        assert!(v.visual_width <= 15);
    }

    // Verify all text was preserved
    let reconstructed: String = visual.iter().map(|v| v.text.as_str()).collect();
    assert_eq!(reconstructed, "the quick brown fox jumps over the lazy dog");
}

#[test]
fn test_offset_and_visual_pos_bidirectional_mapping() {
    let doc = Text::from("hello\nworld");
    let visual = compute_visual_lines(&doc, false, 80, 4);

    // 'h' at offset 0
    let pos0 = doc_offset_to_visual_pos(&visual, 0, 4);
    assert_eq!(pos0, VisualPosition { visual_row: 0, visual_col: 0 });
    assert_eq!(visual_pos_to_doc_offset(&visual, 0, 0, 4), 0);

    // 'w' at offset 6 (after 'hello\n')
    let pos6 = doc_offset_to_visual_pos(&visual, 6, 4);
    assert_eq!(pos6, VisualPosition { visual_row: 1, visual_col: 0 });
    assert_eq!(visual_pos_to_doc_offset(&visual, 1, 0, 4), 6);
}

#[test]
fn test_scroller_autoscroll_vertical_and_horizontal() {
    let mut scroller = Scroller::new(10, 20);
    scroller.margin_vertical = 1;
    scroller.margin_horizontal = 2;

    // Cursor at (0, 0)
    scroller.ensure_visible(VisualPosition { visual_row: 0, visual_col: 0 }, false);
    assert_eq!(scroller.scroll_x, 0);
    assert_eq!(scroller.scroll_y, 0);

    // Cursor moves down to row 15 (beyond visible_lines = 10)
    scroller.ensure_visible(VisualPosition { visual_row: 15, visual_col: 0 }, false);
    assert!(scroller.scroll_y > 0);
    assert!(scroller.visible_rows_range(20).contains(&15));

    // Cursor moves right to column 35 (beyond visible_columns = 20)
    scroller.ensure_visible(VisualPosition { visual_row: 15, visual_col: 35 }, false);
    assert!(scroller.scroll_x > 0);

    // When wrap is enabled, scroll_x must be 0
    scroller.ensure_visible(VisualPosition { visual_row: 15, visual_col: 35 }, true);
    assert_eq!(scroller.scroll_x, 0);
}

#[test]
fn test_scroller_max_lines_restriction() {
    let mut scroller = Scroller::new(50, 80);
    // User configured widget max lines = 10
    scroller.update_dimensions(50, 80, Some(10));
    assert_eq!(scroller.visible_lines, 10);
    assert_eq!(scroller.visible_columns, 80);
}

#[test]
fn test_horizontal_slice() {
    let scroller = Scroller {
        scroll_x: 6,
        visible_columns: 5,
        ..Default::default()
    };
    // Text: "0123456789ABC" -> from col 6 to 11 is "6789A"
    let sliced = scroller.slice_horizontal("0123456789ABC", 4);
    assert_eq!(sliced, "6789A");
}
