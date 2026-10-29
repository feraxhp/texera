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

//! Tests for syntax cache invalidation, lookahead, scrolling, and edit resilience.

use texera_core::TransactionSpec;
use texera_core::Selection;
use ratatui::style::Color;
use texera::HistoryPlugin;
use texera::Highlight;
use texera::LexState;

use super::helpers::assert_cache_consistent;
use super::helpers::state_with_cache;
use super::helpers::comment_depth;
use super::helpers::highlighter;
use super::helpers::plain_lines;
use super::helpers::line_start;
use super::helpers::TEST_LANG;
use super::helpers::stack_at;
use super::helpers::cache;
use super::helpers::entry;
use super::helpers::edit;

#[test]
fn viewport_plus_two_lines_of_lookahead_are_parsed_and_nothing_else() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));

    // Nothing is parsed before anyone asks.
    assert_eq!(cache(&state).stats().lines_lexed, 0);
    assert_eq!(cache(&state).progress().lines_ready, 1);

    h.prepare_viewport(&state, 0..=9);

    // Entry states of lines 0..=11 (visible 0..=9 + 2 lookahead) are known, which required
    // lexing lines 0..=10 only.
    let progress = cache(&state).progress();
    assert_eq!(progress.lines_ready, 12);
    assert_eq!(progress.total_lines, 1000);
    assert_eq!(progress.last_ready_line(), Some(11));
    assert!(!progress.is_complete());
    assert_eq!(cache(&state).stats().lines_lexed, 11);
    assert_eq!(h.parse_progress(&state), Some(progress));
}

#[test]
fn scrolling_down_lexes_only_the_new_line() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));

    h.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).stats().lines_lexed, 11);

    // One new line enters the viewport: exactly one more line gets lexed (lookahead was ready).
    h.prepare_viewport(&state, 1..=10);
    assert_eq!(cache(&state).stats().lines_lexed, 12);
    assert_eq!(cache(&state).progress().lines_ready, 13);

    h.prepare_viewport(&state, 2..=11);
    assert_eq!(cache(&state).stats().lines_lexed, 13);
}

#[test]
fn scrolling_back_up_never_relexes() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));

    h.prepare_viewport(&state, 40..=49);
    let lexed = cache(&state).stats().lines_lexed;
    assert_eq!(lexed, 51);

    h.prepare_viewport(&state, 0..=9);
    h.prepare_viewport(&state, 20..=29);
    h.prepare_viewport(&state, 40..=49);
    assert_eq!(cache(&state).stats().lines_lexed, lexed);
}

#[test]
fn lookahead_is_configurable() {
    let h = highlighter().with_lookahead(0);
    let state = state_with_cache(&plain_lines(100));
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).progress().lines_ready, 10);
}

#[test]
fn parse_stops_at_the_end_of_the_document() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(5));
    h.prepare_viewport(&state, 0..=4);
    let progress = cache(&state).progress();
    assert_eq!(progress.lines_ready, 5);
    assert!(progress.is_complete());
    assert_eq!(progress.ratio(), 1.0);
    assert_eq!(cache(&state).stats().lines_lexed, 4);
}

// ---------------------------------------------------------------------------------------------
// Invalidation tied to the State
// ---------------------------------------------------------------------------------------------

#[test]
fn selection_only_transaction_keeps_the_cache() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(100));
    h.prepare_viewport(&state, 0..=9);

    let next = state
        .update(TransactionSpec::default().with_selection(Selection::cursor(3)))
        .unwrap()
        .into_state();

    assert_eq!(cache(&next).progress().lines_ready, 12);
    assert_eq!(cache(&next).stats().invalidations, 0);
    // Same document, so the memo is literally shared: no lexing needed afterwards either.
    h.prepare_viewport(&next, 0..=9);
    assert_eq!(cache(&next).stats().lines_lexed, 11);
}

#[test]
fn edit_below_the_parsed_range_invalidates_nothing() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));
    h.prepare_viewport(&state, 0..=9);

    let at = line_start(&state, 500);
    let next = edit(state, at, at, "/* comment far below */ ");

    let progress = cache(&next).progress();
    assert_eq!(progress.lines_ready, 12, "parsed range must be untouched");
    assert_eq!(cache(&next).stats().invalidations, 0);
    assert_eq!(cache(&next).stats().lines_lexed, 11);

    h.prepare_viewport(&next, 0..=9);
    assert_eq!(cache(&next).stats().lines_lexed, 11, "nothing to re-lex");
    assert_cache_consistent(&h, &next, "edit below parsed range");
}

#[test]
fn edit_below_the_parsed_frontier_of_a_huge_document() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(10_000));
    h.prepare_viewport(&state, 0..=47);
    assert_eq!(cache(&state).progress().lines_ready, 50);
    let before = cache(&state).stats();

    let at = line_start(&state, 5_000);
    let next = edit(state, at, at, "/* far below the frontier ");

    assert_eq!(cache(&next).stats().invalidations, before.invalidations, "no invalidation");
    assert_eq!(cache(&next).progress().lines_ready, 50, "the first 50 lines are still ready");
    assert_eq!(cache(&next).stats().lines_lexed, before.lines_lexed);
}

#[test]
fn edit_inside_parsed_range_invalidates_only_from_the_edited_line() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).stats().lines_lexed, 11);

    // Open a block comment at the start of line 3.
    let at = line_start(&state, 3);
    let next = edit(state, at, at, "/* ");

    // Entries 0..=3 survive (line 3's *entry* depends only on lines above it).
    assert_eq!(cache(&next).progress().lines_ready, 4);
    assert_eq!(cache(&next).stats().invalidations, 1);

    h.prepare_viewport(&next, 0..=9);

    // Lines 3..=10 had to be lexed again (8 lines), lines 0..=2 were not.
    assert_eq!(cache(&next).stats().lines_lexed, 11 + 8);
    assert_eq!(cache(&next).progress().lines_ready, 12);
    assert_eq!(entry(&h, &next, 3), Some(LexState::BASE));
    for line in 4..12 {
        assert_eq!(
            stack_at(&h, &next, line).unwrap(),
            ["test::root", "test::block_comment"],
            "line {line} must be inside the new comment"
        );
    }
    assert_cache_consistent(&h, &next, "opened comment");

    // Close it again two lines below: entries after the closing line become the base again.
    let at = line_start(&next, 6);
    let closed = edit(next, at, at, "*/ ");
    h.prepare_viewport(&closed, 0..=9);
    assert_eq!(comment_depth(&h, &closed, 6), Some(1));
    assert_eq!(entry(&h, &closed, 7), Some(LexState::BASE));
    assert_cache_consistent(&h, &closed, "closed comment");
}

#[test]
fn edit_that_does_not_change_lexical_state_resyncs_without_relexing_the_tail() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(1000));
    h.prepare_viewport(&state, 0..=9);
    let before = cache(&state).stats();

    // Rename "line 3" -> "renamed 3": same lexical state at the end of the line.
    let from = line_start(&state, 3);
    let next = edit(state, from, from + 4, "renamed");

    assert_eq!(cache(&next).progress().lines_ready, 4, "invalidated right after the edit");

    h.prepare_viewport(&next, 0..=9);

    let after = cache(&next).stats();
    assert_eq!(after.lines_lexed - before.lines_lexed, 1, "only the edited line is lexed");
    assert_eq!(after.resyncs - before.resyncs, 1, "the old tail was restored from the hints");
    assert_eq!(cache(&next).progress().lines_ready, 12, "frontier is back where it was");
    assert_cache_consistent(&h, &next, "state-neutral edit");
}

#[test]
fn local_edit_in_a_fully_parsed_document_lexes_one_line_and_resyncs_once() {
    let h = highlighter().with_parse_budget(usize::MAX);
    let mut lines: Vec<String> = (0..1000).map(|i| format!("line {i}")).collect();
    lines[50] = "let a = 1;".into();
    let state = state_with_cache(&lines.join("\n"));
    h.prepare_viewport(&state, 0..=999);
    assert_eq!(cache(&state).progress().lines_ready, 1000);
    assert!(cache(&state).progress().is_complete());
    let before = cache(&state).stats();

    // `let a = 1;` -> `let abc = 123;`
    let from = line_start(&state, 50);
    let next = edit(state, from, from + "let a = 1;".len(), "let abc = 123;");
    h.prepare_viewport(&next, 0..=999);

    let after = cache(&next).stats();
    assert_eq!(after.invalidations - before.invalidations, 1);
    assert_eq!(after.lines_lexed - before.lines_lexed, 1, "only line 50 is re-scanned");
    assert_eq!(after.resyncs - before.resyncs, 1, "lines 51..1000 are restored in O(1)");
    assert_eq!(cache(&next).progress().lines_ready, 1000);
    assert_cache_consistent(&h, &next, "local edit");
}

#[test]
fn cascade_invalidation_is_bounded_by_the_next_matching_state() {
    let h = highlighter().with_parse_budget(usize::MAX);
    let mut lines: Vec<String> = (0..1000).map(|i| format!("line {i}")).collect();
    lines[20] = "stray */ here".into();
    let state = state_with_cache(&lines.join("\n"));
    h.prepare_viewport(&state, 0..=999);
    assert_eq!(cache(&state).progress().lines_ready, 1000);
    let before = cache(&state).stats();

    // Typing `/*` on line 10 opens a comment that the stray `*/` on line 20 closes.
    let at = line_start(&state, 10);
    let next = edit(state, at, at, "/*");
    h.prepare_viewport(&next, 0..=999);

    let after = cache(&next).stats();
    assert_eq!(after.lines_lexed - before.lines_lexed, 11, "lines 10..=20 only");
    assert_eq!(after.resyncs - before.resyncs, 1);
    assert_eq!(comment_depth(&h, &next, 11), Some(1));
    assert_eq!(comment_depth(&h, &next, 20), Some(1));
    assert_eq!(entry(&h, &next, 21), Some(LexState::BASE));
    assert_eq!(cache(&next).progress().lines_ready, 1000);
    assert_cache_consistent(&h, &next, "bounded cascade");
}

#[test]
fn pressing_enter_three_times_shifts_the_stale_tail() {
    let h = highlighter().with_parse_budget(usize::MAX);
    let state = state_with_cache(&plain_lines(1000));
    h.prepare_viewport(&state, 0..=999);
    let before = cache(&state).stats();

    // Three consecutive `Enter` presses at the start of line 50, *without* parsing in between.
    let mut next = state;
    for _ in 0..3 {
        let at = line_start(&next, 50);
        next = edit(next, at, at, "\n");
    }
    assert_eq!(next.doc().len_lines(), 1003);
    h.prepare_viewport(&next, 0..=1002);

    let after = cache(&next).stats();
    assert_eq!(after.resyncs - before.resyncs, 1, "resynchronised after the shifted tail");
    assert!(after.lines_lexed - before.lines_lexed <= 4, "{} lines lexed", after.lines_lexed - before.lines_lexed);
    assert_eq!(cache(&next).progress().lines_ready, 1003);
    assert_cache_consistent(&h, &next, "three enters");

    // One single transaction inserting the three line breaks behaves the same way.
    let state = state_with_cache(&plain_lines(1000));
    h.prepare_viewport(&state, 0..=999);
    let before = cache(&state).stats();
    let at = line_start(&state, 50);
    let next = edit(state, at, at, "\n\n\n");
    h.prepare_viewport(&next, 0..=1002);
    let after = cache(&next).stats();
    assert_eq!(after.lines_lexed - before.lines_lexed, 4, "the 4 lines of the edited region");
    assert_eq!(after.resyncs - before.resyncs, 1);
    assert_eq!(cache(&next).progress().lines_ready, 1003);
    assert_cache_consistent(&h, &next, "one transaction, three line breaks");
}

#[test]
fn inserting_lines_shifts_the_kept_entries() {
    let h = highlighter();
    let mut lines: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
    lines[8] = "/* start".into();
    lines[10] = "end */".into();
    let state = state_with_cache(&lines.join("\n"));

    h.prepare_viewport(&state, 0..=13);
    assert_eq!(cache(&state).progress().lines_ready, 16);
    let before = cache(&state).stats();

    // Insert an empty line above the comment: everything below moves down by one.
    let at = line_start(&state, 3);
    let next = edit(state, at, at, "\n");
    h.prepare_viewport(&next, 0..=13);

    let after = cache(&next).stats();
    assert_eq!(after.lines_lexed - before.lines_lexed, 2, "only the two lines of the edited region");
    assert_eq!(after.resyncs - before.resyncs, 1);
    // The comment now spans lines 9..=11, so lines 10 and 11 are entered inside the comment.
    assert_eq!(entry(&h, &next, 9), Some(LexState::BASE));
    assert_eq!(comment_depth(&h, &next, 10), Some(1));
    assert_eq!(comment_depth(&h, &next, 11), Some(1));
    assert_eq!(entry(&h, &next, 12), Some(LexState::BASE));
    assert_cache_consistent(&h, &next, "inserted line");

    // And removing it again (deleting the newline) goes back.
    let back = edit(next, at, at + 1, "");
    h.prepare_viewport(&back, 0..=13);
    assert_cache_consistent(&h, &back, "removed line");
    assert_eq!(comment_depth(&h, &back, 9), Some(1));
}

#[test]
fn undo_and_redo_derive_valid_caches() {
    let h = highlighter();
    let mut state = state_with_cache(&plain_lines(200));
    let original = state.doc().to_string();
    h.prepare_viewport(&state, 0..=9);

    // Edit: open a block comment on line 3.
    let at = line_start(&state, 3);
    state = edit(state, at, at, "/* ");
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(comment_depth(&h, &state, 8), Some(1));
    let edited = state.doc().to_string();

    // Undo restores the text *and* the lexical states.
    assert!(HistoryPlugin::undo(&mut state));
    assert_eq!(state.doc().to_string(), original);
    assert!(cache(&state).progress().lines_ready < 12, "undo invalidates the edited region");
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(entry(&h, &state, 8), Some(LexState::BASE));
    assert_cache_consistent(&h, &state, "after undo");

    // Redo brings the comment (and the states) back.
    assert!(HistoryPlugin::redo(&mut state));
    assert_eq!(state.doc().to_string(), edited);
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(comment_depth(&h, &state, 8), Some(1));
    assert_cache_consistent(&h, &state, "after redo");
}

#[test]
fn the_previous_state_keeps_its_own_cache_intact() {
    let h = highlighter();
    let before = state_with_cache(&plain_lines(300));
    h.prepare_viewport(&before, 0..=19);
    let known = cache(&before).known_states();
    let stats = cache(&before).stats();

    // Typing `/*` derives a new state with its own cache...
    let at = line_start(&before, 5);
    let after = edit(before.clone(), at, at, "/*");
    h.prepare_viewport(&after, 0..=19);
    assert_eq!(comment_depth(&h, &after, 10), Some(1));

    // ...and the previous one is untouched: nothing to re-parse when we go back to it.
    assert_eq!(cache(&before).known_states(), known);
    assert_eq!(cache(&before).stats(), stats);
    h.prepare_viewport(&before, 0..=19);
    assert_eq!(cache(&before).stats(), stats, "no lexing needed");
    assert_eq!(comment_depth(&h, &before, 10), Some(0));
}

#[test]
fn changing_the_language_discards_the_cached_parse() {
    let h = highlighter();
    let state = state_with_cache(&plain_lines(100));
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).stats().lines_lexed, 11);
    let old_id = h.rules_id().unwrap();
    assert_eq!(cache(&state).entry_state(5, old_id), Some(LexState::BASE));

    // Re-registering the language bumps the revision of the (shared) registry.
    let h2 = h.clone().with_language("test", TEST_LANG, None);
    let new_id = h2.rules_id().unwrap();
    assert_ne!(old_id, new_id);
    assert_eq!(cache(&state).entry_state(5, new_id), None, "old entries do not belong to the new rules");

    h2.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).stats().lines_lexed, 22, "parsed again with the new rules");
    assert_eq!(cache(&state).stats().invalidations, 1);
}

// ---------------------------------------------------------------------------------------------
// Budget and observable progress
// ---------------------------------------------------------------------------------------------

#[test]
fn per_frame_budget_on_a_huge_document() {
    let h = highlighter().with_parse_budget(5_000);
    let state = state_with_cache(&plain_lines(20_000));

    // The user jumps straight to the end of the file.
    h.prepare_viewport(&state, 19_950..=20_000);
    assert_eq!(cache(&state).progress().lines_ready, 5_001);
    assert_eq!(cache(&state).stats().lines_lexed, 5_000);
    assert!(!cache(&state).progress().is_complete());

    h.prepare_viewport(&state, 19_950..=20_000);
    assert_eq!(cache(&state).progress().lines_ready, 10_001);
    h.prepare_viewport(&state, 19_950..=20_000);
    assert_eq!(cache(&state).progress().lines_ready, 15_001);
    assert!(!h.parse_progress(&state).unwrap().is_complete());

    h.prepare_viewport(&state, 19_950..=20_000);
    let progress = h.parse_progress(&state).unwrap();
    assert!(progress.is_complete(), "complete on the fourth call");
    assert_eq!(progress.lines_ready, 20_000);
    assert_eq!(cache(&state).stats().lines_lexed, 19_999);
}

#[test]
fn far_jump_progress_is_observable_and_bounded_by_the_budget() {
    let h = highlighter().with_parse_budget(500);
    let state = state_with_cache(&plain_lines(3000));

    // The user jumps to line 2500. Each frame lexes at most `budget` lines.
    let mut last = cache(&state).progress().lines_ready;
    let mut frames = 0;
    while !cache(&state).progress().is_ready(2509) {
        let lexed_before = cache(&state).stats().lines_lexed;
        h.prepare_viewport(&state, 2500..=2507);
        let progress = h.parse_progress(&state).unwrap();

        assert!(progress.lines_ready > last, "every frame must make progress");
        assert!(cache(&state).stats().lines_lexed - lexed_before <= 500, "budget respected");
        assert!(!progress.is_ready(2509) || progress.lines_ready >= 2510);
        last = progress.lines_ready;
        frames += 1;
        assert!(frames < 20, "must converge");
    }
    assert_eq!(frames, 6, "2509 lines / 500 per frame");
    assert_eq!(cache(&state).progress().lines_ready, 2510);
    // Nothing past the lookahead was touched.
    assert_eq!(cache(&state).stats().lines_lexed, 2509);
}

#[test]
fn lines_beyond_the_frontier_fall_back_without_panicking() {
    let h = highlighter().with_parse_budget(10);
    let text = format!("/* open\n{}", plain_lines(200));
    let state = state_with_cache(&text);
    h.prepare_viewport(&state, 150..=155);

    // Line 150 is not ready: stateless fallback (no comment context), but still valid output.
    assert!(!cache(&state).progress().is_ready(150));
    let _ = h.highlight_document_line(&state, 150, "line 149");

    // Once the parse catches up, the same call knows the line is inside the comment.
    while !cache(&state).progress().is_ready(155) {
        h.prepare_viewport(&state, 150..=155);
    }
    let styled = h.highlight_document_line(&state, 150, "line 149");
    assert!(styled.iter().any(|(_, s)| s.fg == Some(Color::DarkGray)));
}
