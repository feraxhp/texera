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

//! Property fuzzing and rendering integration tests for `SyntaxCache`.

use ratatui::widgets::StatefulWidget;
use texera_core::TransactionSpec;
use ratatui::buffer::Buffer;
use texera::EditorViewState;
use texera::HighlightPlugin;
use texera_core::ChangeSpec;
use ratatui::layout::Rect;
use ratatui::style::Color;
use texera::HistoryPlugin;
use texera::EditorView;
use texera_core::State;
use texera::Highlight;
use texera_core::Text;
use texera::ThemeMap;

use super::helpers::assert_cache_consistent;
use super::helpers::state_with_cache;
use super::helpers::highlighter;
use super::helpers::plain_lines;
use super::common::MARKDOWN;
use super::helpers::cache;
use super::helpers::edit;
use super::common::JSON;
use super::common::RUST;
use super::common::Rng;

// ---------------------------------------------------------------------------------------------
// Property test: incremental == full parse
// ---------------------------------------------------------------------------------------------

fn fuzz(h: &HighlightPlugin, snippets: &[&str], seeds: std::ops::RangeInclusive<u64>, initial_len: usize) -> u64 {
    let h = h.clone().with_parse_budget(7);
    let mut total_resyncs = 0;

    for seed in seeds {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let initial: String = (0..initial_len).map(|_| snippets[rng.below(snippets.len())]).collect();
        let mut state = state_with_cache(&initial);

        for step in 0..60 {
            // 1 or 2 concurrent, non-overlapping edits (multi-cursor style).
            let len = state.doc().len_chars();
            let mut cuts = [rng.below(len + 1), rng.below(len + 1)];
            cuts.sort();
            let mut spec = TransactionSpec::default();
            let first_to = (cuts[0] + rng.below(3)).min(cuts[1]).min(len);
            spec = spec.with_change(
                ChangeSpec::new(cuts[0], Some(first_to), Some(Text::from(snippets[rng.below(snippets.len())]))).unwrap(),
            );
            if rng.below(2) == 0 && cuts[1] > first_to {
                let to = (cuts[1] + rng.below(3)).min(len);
                spec = spec.with_change(
                    ChangeSpec::new(cuts[1], Some(to), Some(Text::from(snippets[rng.below(snippets.len())]))).unwrap(),
                );
            }
            state = state.update(spec).unwrap().into_state();

            // The viewport moves randomly, so the frontier is sometimes below and sometimes above
            // the edit.
            if rng.below(3) != 0 {
                let lines = state.doc().len_lines();
                let start = rng.below(lines);
                h.prepare_viewport(&state, start..=(start + rng.below(6)).min(lines - 1));
            }

            assert_cache_consistent(&h, &state, &format!("seed {seed} step {step}"));
        }

        // Finally parse everything and compare in full.
        let total = state.doc().len_lines();
        while !cache(&state).progress().is_complete() {
            h.prepare_viewport(&state, 0..=total);
        }
        assert_cache_consistent(&h, &state, &format!("seed {seed} final"));
        assert_eq!(cache(&state).known_states().len(), total);
        total_resyncs += cache(&state).stats().resyncs;
    }
    total_resyncs
}

#[test]
fn incremental_cache_always_equals_a_full_parse() {
    const SNIPPETS: [&str; 12] = ["/*", "*/", "\"", "\n", "\n", "a", "//", "'", "\\", " ", "é", "x = 1;"];
    let resyncs = fuzz(&highlighter(), &SNIPPETS, 1..=40, 120);
    // The scenario must actually exercise the resynchronisation path, not only plain re-lexing.
    assert!(resyncs > 0, "fuzzing never triggered a resync");
}

#[test]
fn incremental_cache_equals_a_full_parse_with_embedded_languages() {
    // Deep stacks (markdown > rust > fn_body > string > interpolation > ...) must resynchronise
    // exactly like flat states do.
    const SNIPPETS: [&str; 20] = [
        "```rust\n", "```\n", "```json\n", "```\n", "fn f() {", "}", "/*", "*/", "\"", "${", "\n", "\n", "let x = 1;", "{", "[",
        "\"k\":", "# h\n", "é", " ", "x(",
    ];
    let h = HighlightPlugin::new()
        .with_language("markdown", MARKDOWN, None)
        .with_language("rust", RUST, None)
        .with_language("json", JSON, None);
    let resyncs = fuzz(&h, &SNIPPETS, 1..=40, 60);
    assert!(resyncs > 0, "fuzzing never triggered a resync");
}

// ---------------------------------------------------------------------------------------------
// Rendering integration
// ---------------------------------------------------------------------------------------------

fn render(view: &EditorView, state: &State, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    let mut view_state = EditorViewState::new();
    view.bind(state).render(area, &mut buf, &mut view_state);
    buf
}

#[test]
fn render_colors_lines_inside_a_multiline_comment() {
    let text = "/* open\nstill comment\n*/ let x = 1;\nlet y = 2;";
    let state = state_with_cache(text);
    let view = EditorView::new().with_highlighter(highlighter());
    let buf = render(&view, &state, 30, 6);

    // Row 1 is wholly inside the comment opened on row 0.
    assert_eq!(buf[(6, 1)].style().fg, Some(Color::DarkGray));
    // Row 2 starts with the comment's end...
    assert_eq!(buf[(0, 2)].style().fg, Some(Color::DarkGray));
    // ...and row 3 is regular code again (no theme color for it -> not comment gray).
    assert_ne!(buf[(5, 3)].style().fg, Some(Color::DarkGray));

    // Without the SyntaxField the line is lexed statelessly: row 1 is not recognised.
    let plain_state = State::from(text);
    let buf = render(&view, &plain_state, 30, 6);
    assert_ne!(buf[(6, 1)].style().fg, Some(Color::DarkGray));
}

#[test]
fn render_only_parses_what_is_visible() {
    let state = state_with_cache(&plain_lines(5000));
    let view = EditorView::new().with_highlighter(highlighter());
    let _ = render(&view, &state, 40, 5);

    // 5 visible rows (lines 0..=4) + 2 lookahead -> entries 0..=6, i.e. 6 lexed lines.
    assert_eq!(cache(&state).progress().lines_ready, 7);
    assert_eq!(cache(&state).stats().lines_lexed, 6);

    // Rendering again is free.
    let _ = render(&view, &state, 40, 5);
    assert_eq!(cache(&state).stats().lines_lexed, 6);
}

#[test]
fn render_wrapped_rows_share_the_logical_line_state() {
    // The second visual row of a wrapped comment line must still be a comment.
    let text = "// aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nlet x = 1;";
    let state = state_with_cache(text);
    let view = EditorView::new().with_wrap(true).with_highlighter(highlighter());
    let buf = render(&view, &state, 10, 6);

    assert_eq!(buf[(1, 0)].style().fg, Some(Color::DarkGray));
    assert_eq!(buf[(5, 1)].style().fg, Some(Color::DarkGray), "wrapped continuation");
    assert_eq!(buf[(5, 2)].style().fg, Some(Color::DarkGray), "wrapped continuation");
}

#[test]
fn render_after_undo_uses_the_restored_states() {
    let text = "let a = 1;\nlet b = 2;\nlet c = 3;";
    let mut state = state_with_cache(text);
    let view = EditorView::new().with_highlighter(highlighter());
    let _ = render(&view, &state, 30, 5);

    state = edit(state, 0, 0, "/* ");
    let buf = render(&view, &state, 30, 5);
    assert_eq!(buf[(5, 1)].style().fg, Some(Color::DarkGray), "commented out");

    assert!(HistoryPlugin::undo(&mut state));
    let buf = render(&view, &state, 30, 5);
    assert_ne!(buf[(5, 1)].style().fg, Some(Color::DarkGray), "comment gone after undo");
}

#[test]
fn view_state_visible_lines_and_syntax_ready_helpers() {
    let state = state_with_cache(&plain_lines(100));
    let mut vs = EditorViewState::new();
    vs.scroller.update_dimensions(20, 80, None);

    assert_eq!(vs.visible_lines(100), 0..20);
    // Initially only line 0 is ready, so visible lines 0..20 are not fully ready
    assert!(!vs.syntax_ready(&state));

    let h = highlighter();
    h.prepare_viewport(&state, 0..=19);
    // Now lines 0..20 are parsed
    assert!(vs.syntax_ready(&state));
}

#[test]
fn fast_unwrapped_viewport_render_matches_expected_content() {
    let text = plain_lines(500);
    let state = state_with_cache(&text);
    let view = EditorView::new().with_wrap(false).with_highlighter(highlighter());
    let buf = render(&view, &state, 30, 10);

    // Row 0 should be "line 0"
    let line0: String = (0..6).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    assert_eq!(line0, "line 0");

    // Row 9 should be "line 9"
    let line9: String = (0..6).map(|x| buf[(x, 9)].symbol().to_string()).collect();
    assert_eq!(line9, "line 9");
}

#[test]
fn render_colors_embedded_code_blocks() {
    let mut theme = ThemeMap::default();
    theme.set_color("keyword.function", Color::Magenta);
    theme.set_color("markup.heading", Color::Yellow);
    let h = HighlightPlugin::new()
        .with_language("markdown", MARKDOWN, Some(theme.clone()))
        .with_language("rust", RUST, Some(theme));
    let text = "# T\n```rust\nfn main() {\n}\n```\nfn not code";
    let state = state_with_cache(text);
    let view = EditorView::new().with_highlighter(h);
    let buf = render(&view, &state, 30, 8);

    assert_eq!(buf[(0, 0)].style().fg, Some(Color::Yellow));
    assert_eq!(buf[(0, 2)].style().fg, Some(Color::Magenta), "`fn` inside the rust block");
    assert_ne!(buf[(0, 5)].style().fg, Some(Color::Magenta), "`fn` after the block");
}
