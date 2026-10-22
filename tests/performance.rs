use std::time::Instant;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use texera::{
    apply_action, render_editor, EditorAction, EditorView, EditorViewState, HistoryPlugin,
};
use texera_core::{tr, AllowMultipleSelections, Selection, State};

#[test]
fn test_multicursor_1000_operations_perf() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create a document with 1500 lines
    let mut doc_text = String::new();
    for i in 0..1500 {
        doc_text.push_str(&format!("let x_{} = \"some value for testing performance with 1k cursors\";\n", i));
    }

    let mut state = State::from(doc_text.as_str())
        .with_extensions([AllowMultipleSelections(true).into()]);

    // 2. Create 1,000 cursors (one per line)
    let t_create = Instant::now();
    let mut ranges = Vec::with_capacity(1000);
    for i in 0..1000 {
        let line = state.doc().line(i)?;
        ranges.push(Selection::cursor(line.from() + 4));
    }
    let sel = Selection::from_with_head(ranges, 0)?;
    state = state.update(tr!({ selection: sel }))?.into_state();
    let create_elapsed = t_create.elapsed();
    println!("PERF: Created 1000 cursors in {:?}", create_elapsed);
    assert_eq!(state.selection().ranges_ref().len(), 1000);

    // 3. Measure Navigation with 1,000 cursors
    let t_move_right = Instant::now();
    apply_action(&mut state, EditorAction::MoveRight, None)?;
    let move_right_elapsed = t_move_right.elapsed();
    println!("PERF: MoveRight with 1000 cursors in {:?}", move_right_elapsed);

    let t_move_left = Instant::now();
    apply_action(&mut state, EditorAction::MoveLeft, None)?;
    let move_left_elapsed = t_move_left.elapsed();
    println!("PERF: MoveLeft with 1000 cursors in {:?}", move_left_elapsed);

    let t_line_end = Instant::now();
    apply_action(&mut state, EditorAction::MoveLineEnd, None)?;
    let line_end_elapsed = t_line_end.elapsed();
    println!("PERF: MoveLineEnd with 1000 cursors in {:?}", line_end_elapsed);

    let t_line_start = Instant::now();
    apply_action(&mut state, EditorAction::MoveLineStart, None)?;
    let line_start_elapsed = t_line_start.elapsed();
    println!("PERF: MoveLineStart with 1000 cursors in {:?}", line_start_elapsed);

    // 4. Measure Frame Rendering with 1,000 cursors
    let view = EditorView::new();
    let mut view_state = EditorViewState::default();
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 30));

    let t_render = Instant::now();
    render_editor(&view, &state, Rect::new(0, 0, 100, 30), &mut buf, &mut view_state);
    let render_elapsed = t_render.elapsed();
    println!("PERF: Render 1 frame (100x30) with 1000 cursors in {:?}", render_elapsed);

    // Average over 20 consecutive frames
    let t_renders_20 = Instant::now();
    for _ in 0..20 {
        render_editor(&view, &state, Rect::new(0, 0, 100, 30), &mut buf, &mut view_state);
    }
    let avg_render_elapsed = t_renders_20.elapsed() / 20;
    println!("PERF: Average frame render (20 frames) in {:?}", avg_render_elapsed);

    // 5. Measure Typing and Deleting with 1,000 cursors
    let t_type = Instant::now();
    let key_ev = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::empty());
    apply_action(&mut state, EditorAction::InsertChar, Some(&key_ev))?;
    let type_elapsed = t_type.elapsed();
    println!("PERF: InsertChar 'y' across 1000 cursors in {:?}", type_elapsed);

    let t_del = Instant::now();
    apply_action(&mut state, EditorAction::DeleteBackward, None)?;
    let del_elapsed = t_del.elapsed();
    println!("PERF: DeleteBackward across 1000 cursors in {:?}", del_elapsed);

    // Basic sanity checks
    assert_eq!(state.selection().ranges_ref().len(), 1000);
    assert_eq!(state.doc().len_lines(), 1501);

    Ok(())
}

#[test]
fn test_newline_insertion_curve_flatness() -> Result<(), Box<dyn std::error::Error>> {
    // Tests that continuously holding Enter to add new lines doesn't suffer from
    // quadratic degradation around 450-600 lines or erratic spikes.
    let mut state = State::from("")
        .with_extensions([
            HistoryPlugin::extension(None),
            AllowMultipleSelections(true).into(),
        ]);

    let view = EditorView::new();
    let mut view_state = EditorViewState::default();
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));

    let mut chunk_500_time = std::time::Duration::ZERO;
    let mut chunk_1050_time = std::time::Duration::ZERO;

    let mut chunk_start = Instant::now();
    for i in 1..=1100 {
        apply_action(&mut state, EditorAction::InsertNewline, None)?;
        render_editor(&view, &state, Rect::new(0, 0, 80, 24), &mut buf, &mut view_state);

        if i == 550 {
            chunk_500_time = chunk_start.elapsed();
            chunk_start = Instant::now();
        } else if i == 500 || i == 600 || i == 1000 {
            chunk_start = Instant::now();
        } else if i == 1050 {
            chunk_1050_time = chunk_start.elapsed();
            chunk_start = Instant::now();
        }
    }

    println!(
        "PERF: 50 newlines + render at lines 500..550: {:?} | at lines 1000..1050: {:?}",
        chunk_500_time, chunk_1050_time
    );

    // Both 50-step chunks must remain fast and within healthy bounds
    assert!(
        chunk_500_time.as_millis() < 50,
        "Lines 500..550 should not degrade quadratically! Took {:?}",
        chunk_500_time
    );
    assert!(
        chunk_1050_time.as_millis() < 50,
        "Lines 1000..1050 should remain fast! Took {:?}",
        chunk_1050_time
    );

    assert_eq!(state.doc().len_lines(), 1101);
    Ok(())
}
