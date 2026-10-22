use texera::{EditorView, EditorViewState};

#[cfg(feature = "highlight")]
use ratatui::style::Color;
#[cfg(feature = "highlight")]
use texera::{HighlightPlugin, HistoryPlugin, LineNumberPlugin, ThemeMap, UnsavedLinePlugin};

#[test]
fn test_editor_view_builder_minimal() {
    let view = EditorView::new();
    let state = EditorViewState::new();
    assert_eq!(state.scroll_x(), 0);
    assert_eq!(state.scroll_y(), 0);
    assert!(state.last_area.is_none());
    drop(view);
}

#[cfg(feature = "highlight")]
#[test]
fn test_editor_view_builder_with_plugins() {
    let mut unsaved = UnsavedLinePlugin::new();
    unsaved.mark_line_dirty(3);
    assert!(unsaved.is_line_dirty(3));
    assert!(!unsaved.is_line_dirty(1));

    let highlight = HighlightPlugin::with_specs(("dummy_spec", None));

    let view = EditorView::new()
        .with_plugin(LineNumberPlugin::default())
        .with_plugin(unsaved)
        .with_plugin(HistoryPlugin::default())
        .with_highlighter(highlight)
        .with_max_lines(20)
        .with_wrap(false);

    drop(view);
}

#[cfg(feature = "highlight")]
#[test]
fn test_spec_theme_json_parsing() {
    let json = r##"{
        "string": "green",
        "keyword": "#f6c99f",
        "number": "magenta"
    }"##;

    let theme = ThemeMap::from_json_str(json).expect("Theme JSON should parse");
    assert_eq!(theme.color_for("string"), Some(Color::Green));
    assert_eq!(theme.color_for("number"), Some(Color::Magenta));
    assert_eq!(theme.color_for("keyword"), Some(Color::Rgb(246, 201, 159)));
    assert_eq!(theme.color_for("non_existent"), None);
}
