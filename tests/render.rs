use texera_core::AllowMultipleSelections;
use ratatui::widgets::StatefulWidget;
use ratatui::style::Modifier;
use texera::GlyphTransformer;
use texera::LineNumberPlugin;
use texera::TransformedGlyph;
use ratatui::buffer::Buffer;
use texera::EditorViewState;
use texera_core::Selection;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use texera::GlyphContext;
use texera_core::TabSize;
use texera::EditorView;
use texera::ViewPlugin;
use texera_core::State;

#[test]
fn render_minimal_without_gutter() {
    let state = State::from("Hello, Texera!");
    let view = EditorView::new(); // No gutter by default

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
fn render_with_line_gutter() {
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
fn render_text_selection() {
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
fn render_multiple_cursors() {
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
fn render_soft_wrapping() {
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


// Helper transformers for rendering tests

#[derive(Clone)]
struct SpaceToDotTransformer;

impl ViewPlugin for SpaceToDotTransformer {}

impl GlyphTransformer for SpaceToDotTransformer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        if ctx.ch == ' ' {
            Some(TransformedGlyph::Char('·', None))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct TabArrowTransformer;

impl ViewPlugin for TabArrowTransformer {}

impl GlyphTransformer for TabArrowTransformer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        if ctx.ch == '\t' {
            Some(TransformedGlyph::Tab {
                filler: '─',
                terminator: '→',
                style: None,
            })
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct StyledSpaceTransformer(Color);

impl ViewPlugin for StyledSpaceTransformer {}

impl GlyphTransformer for StyledSpaceTransformer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        if ctx.ch == ' ' {
            Some(TransformedGlyph::Char('·', Some(Style::default().fg(self.0))))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct CellsTabTransformer;

impl ViewPlugin for CellsTabTransformer {}

impl GlyphTransformer for CellsTabTransformer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        if ctx.ch == '\t' {
            Some(TransformedGlyph::Cells(vec!['<', '-', '-', '>'], None))
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct FacetCheckingTransformer;

impl ViewPlugin for FacetCheckingTransformer {}

impl GlyphTransformer for FacetCheckingTransformer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        if ctx.ch == '\t' {
            assert_eq!(ctx.state.tab_size(), 2);
            assert_eq!(ctx.cell_width, 2);
            Some(TransformedGlyph::Tab {
                filler: '─',
                terminator: '→',
                style: None,
            })
        } else {
            None
        }
    }
}

#[test]
fn glyph_transformer_space_to_dot() {
    let state = State::from("hello world");
    let view = EditorView::new().with_glyph_transformer(SpaceToDotTransformer);

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(5, 0)].symbol(), "·");
    assert_eq!(buf[(0, 0)].symbol(), "h");
    assert_eq!(buf[(6, 0)].symbol(), "w");
}

#[test]
fn glyph_transformer_tab_with_arrow() {
    let state = State::from("\tfoo");
    let view = EditorView::new().with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "─");
    assert_eq!(buf[(1, 0)].symbol(), "─");
    assert_eq!(buf[(2, 0)].symbol(), "─");
    assert_eq!(buf[(3, 0)].symbol(), "→");
    assert_eq!(buf[(4, 0)].symbol(), "f");
}

#[test]
fn glyph_transformer_tab_width_one() {
    let state = State::from("abc\t");
    let view = EditorView::new().with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "a");
    assert_eq!(buf[(1, 0)].symbol(), "b");
    assert_eq!(buf[(2, 0)].symbol(), "c");
    assert_eq!(buf[(3, 0)].symbol(), "→");
}

#[test]
fn glyph_transformer_tab_width_two() {
    let state = State::from("ab\t");
    let view = EditorView::new().with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "a");
    assert_eq!(buf[(1, 0)].symbol(), "b");
    assert_eq!(buf[(2, 0)].symbol(), "─");
    assert_eq!(buf[(3, 0)].symbol(), "→");
}

#[test]
fn glyph_transformer_tab_width_three() {
    let state = State::from("a\t");
    let view = EditorView::new().with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 20, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "a");
    assert_eq!(buf[(1, 0)].symbol(), "─");
    assert_eq!(buf[(2, 0)].symbol(), "─");
    assert_eq!(buf[(3, 0)].symbol(), "→");
}

#[test]
fn glyph_transformer_custom_style() {
    let state = State::from("a b");
    let view = EditorView::new().with_glyph_transformer(StyledSpaceTransformer(Color::Cyan));

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(1, 0)].symbol(), "·");
    assert_eq!(buf[(1, 0)].style().fg, Some(Color::Cyan));
}

#[test]
fn glyph_transformer_preserves_selection_background() {
    let selection = Selection::from([Selection::range(0, 3)]);
    let state = State::with_selection("a b", selection);
    let view = EditorView::new().with_glyph_transformer(SpaceToDotTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(1, 0)].symbol(), "·");
    assert_eq!(buf[(1, 0)].style().bg, Some(Color::DarkGray));
}

#[test]
fn glyph_transformer_preserves_cursor_style() {
    let selection = Selection::from([Selection::cursor(1)]);
    let state = State::with_selection("a b", selection);
    let view = EditorView::new().with_glyph_transformer(SpaceToDotTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(1, 0)].symbol(), "·");
    assert!(buf[(1, 0)].style().add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn glyph_transformer_fallback_none() {
    let state = State::from("a\tb c");
    let view = EditorView::new().with_glyph_transformer(SpaceToDotTransformer);

    let area = Rect::new(0, 0, 15, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "a");
    // Tab rendered as default spaces
    for col in 1..4 {
        assert_eq!(buf[(col, 0)].symbol(), " ");
    }
    assert_eq!(buf[(4, 0)].symbol(), "b");
    assert_eq!(buf[(5, 0)].symbol(), "·");
    assert_eq!(buf[(6, 0)].symbol(), "c");
}

#[test]
fn glyph_transformer_multiple_chain() {
    let state = State::from(" \t");
    let view = EditorView::new()
        .with_glyph_transformer(SpaceToDotTransformer)
        .with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "·");
    assert_eq!(buf[(1, 0)].symbol(), "─");
    assert_eq!(buf[(2, 0)].symbol(), "─");
    assert_eq!(buf[(3, 0)].symbol(), "→");
}

#[test]
fn glyph_transformer_with_horizontal_scroll() {
    // Put cursor at the end of the text (offset 5) so autoscroll doesn't reset scroll_x to 0
    let state = State::with_selection("\ttext", Selection::from([Selection::cursor(5)]));
    let view = EditorView::new().with_glyph_transformer(TabArrowTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();
    view_state.set_scroll(2, 0);

    view.bind(&state).render(area, &mut buf, &mut view_state);

    // Tab spans visual columns 0..4 ('─', '─', '─', '→').
    // With scroll_x = 2:
    // Screen column 0 corresponds to visual col 2 ('─')
    // Screen column 1 corresponds to visual col 3 ('→')
    // Screen columns 2..6 correspond to "text"
    assert_eq!(buf[(0, 0)].symbol(), "─");
    assert_eq!(buf[(1, 0)].symbol(), "→");
    assert_eq!(buf[(2, 0)].symbol(), "t");
    assert_eq!(buf[(3, 0)].symbol(), "e");
    assert_eq!(buf[(4, 0)].symbol(), "x");
    assert_eq!(buf[(5, 0)].symbol(), "t");
}

#[test]
fn glyph_transformer_cells_variant() {
    let state = State::from("\t");
    let view = EditorView::new().with_glyph_transformer(CellsTabTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "<");
    assert_eq!(buf[(1, 0)].symbol(), "-");
    assert_eq!(buf[(2, 0)].symbol(), "-");
    assert_eq!(buf[(3, 0)].symbol(), ">");
}

#[test]
fn glyph_transformer_reads_state_tab_size_facet() {
    let state = State::from("\t").with_extensions(TabSize(2));
    let view = EditorView::new().with_glyph_transformer(FacetCheckingTransformer);

    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();

    view.bind(&state).render(area, &mut buf, &mut view_state);

    assert_eq!(buf[(0, 0)].symbol(), "─");
    assert_eq!(buf[(1, 0)].symbol(), "→");
    assert_eq!(buf[(2, 0)].symbol(), " ");
}
