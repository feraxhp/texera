//! Category 3: incrementality and resynchronisation of the per-document `SyntaxCache`, plus its
//! integration with rendering.

mod common;

use common::{MARKDOWN, RUST, Rng};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::StatefulWidget;
use texera::{
    EditorView, EditorViewState, Highlight, HighlightPlugin, HistoryPlugin, LexState, ThemeMap, SyntaxCache, SyntaxField,
};
use texera_core::{ChangeSpec, Selection, State, Text, TransactionSpec};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// `//` comments, nestable multi-line `/* */` comments and multi-line `"..."` strings.
const TEST_LANG: &str = r#"
[meta]
name = "test"

[states.root]
rules = [
    { match = '//.*', token = "comment" },
    { match = '/\*', token = "comment", push = "block_comment" },
    { match = '"', token = "string", push = "string" },
]

[states.string]
default_token = "string"
rules = [
    { match = '\\.', token = "string" },
    { match = '"', token = "string", pop = true },
]

[states.block_comment]
default_token = "comment"
rules = [
    { match = '/\*', token = "comment", push = "block_comment" },
    { match = '\*/', token = "comment", pop = true },
]
"#;

fn highlighter() -> HighlightPlugin {
    let mut theme = ThemeMap::default();
    theme.set_color("comment", Color::DarkGray);
    theme.set_color("string", Color::Green);
    HighlightPlugin::new().with_language("test", TEST_LANG, Some(theme))
}

fn plain_lines(n: usize) -> String {
    (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

fn state_with_cache(text: &str) -> State {
    State::from(text).with_extensions([HighlightPlugin::syntax_extension(), HistoryPlugin::extension(None)])
}

fn cache(state: &State) -> &SyntaxCache {
    state.field::<SyntaxField>().expect("SyntaxField registered")
}

fn line_start(state: &State, line: usize) -> usize {
    state.doc().line(line).unwrap().from()
}

/// Replaces `from..to` (char offsets) with `insert`.
fn edit(state: State, from: usize, to: usize, insert: &str) -> State {
    let spec = TransactionSpec::default()
        .with_change(ChangeSpec::new(from, Some(to), Some(Text::from(insert))).unwrap());
    state.update(spec).unwrap().into_state()
}

/// Entry states of the whole document computed from scratch by a brand-new cache. The registry is
/// shared with `h`, so equal stacks have equal handles.
fn fresh_states(h: &HighlightPlugin, text: &str) -> Vec<LexState> {
    let h = h.clone().with_parse_budget(usize::MAX);
    let state = state_with_cache(text);
    let total = state.doc().len_lines();
    h.prepare_viewport(&state, 0..=total);
    cache(&state).known_states()
}

/// Every verified entry of the cache must equal what a from-scratch parse produces.
fn assert_cache_consistent(h: &HighlightPlugin, state: &State, context: &str) {
    let known = cache(state).known_states();
    let fresh = fresh_states(h, &state.doc().to_string());
    assert!(known.len() <= fresh.len(), "{context}: cache knows more lines than the doc has");
    assert_eq!(known, fresh[..known.len()], "{context}: cached states diverge from a fresh parse");
}

/// Entry state of `line` as the readable stack, e.g. `["test::root", "test::block_comment"]`.
fn stack_at(h: &HighlightPlugin, state: &State, line: usize) -> Option<Vec<String>> {
    let (lang, _) = h.active().unwrap();
    cache(state)
        .entry_state(line, h.rules_id().unwrap())
        .map(|s| h.registry().describe(s, lang))
}

fn comment_depth(h: &HighlightPlugin, state: &State, line: usize) -> Option<usize> {
    stack_at(h, state, line).map(|s| s.iter().filter(|f| f.ends_with("block_comment")).count())
}

fn entry(h: &HighlightPlugin, state: &State, line: usize) -> Option<LexState> {
    cache(state).entry_state(line, h.rules_id().unwrap())
}

// ---------------------------------------------------------------------------------------------
// Only parse what is needed
// ---------------------------------------------------------------------------------------------

#[test]
fn test_viewport_plus_two_lines_of_lookahead_are_parsed_and_nothing_else() {
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
fn test_scrolling_down_lexes_only_the_new_line() {
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
fn test_scrolling_back_up_never_relexes() {
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
fn test_lookahead_is_configurable() {
    let h = highlighter().with_lookahead(0);
    let state = state_with_cache(&plain_lines(100));
    h.prepare_viewport(&state, 0..=9);
    assert_eq!(cache(&state).progress().lines_ready, 10);
}

#[test]
fn test_parse_stops_at_the_end_of_the_document() {
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
fn test_selection_only_transaction_keeps_the_cache() {
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
fn test_edit_below_the_parsed_range_invalidates_nothing() {
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
fn test_edit_below_the_parsed_frontier_of_a_huge_document() {
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
fn test_edit_inside_parsed_range_invalidates_only_from_the_edited_line() {
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
fn test_edit_that_does_not_change_lexical_state_resyncs_without_relexing_the_tail() {
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
fn test_local_edit_in_a_fully_parsed_document_lexes_one_line_and_resyncs_once() {
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
fn test_cascade_invalidation_is_bounded_by_the_next_matching_state() {
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
fn test_pressing_enter_three_times_shifts_the_stale_tail() {
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
fn test_inserting_lines_shifts_the_kept_entries() {
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
fn test_undo_and_redo_derive_valid_caches() {
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
fn test_the_previous_state_keeps_its_own_cache_intact() {
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
fn test_changing_the_language_discards_the_cached_parse() {
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
fn test_per_frame_budget_on_a_huge_document() {
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
fn test_far_jump_progress_is_observable_and_bounded_by_the_budget() {
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
fn test_lines_beyond_the_frontier_fall_back_without_panicking() {
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
fn test_incremental_cache_always_equals_a_full_parse() {
    const SNIPPETS: [&str; 12] = ["/*", "*/", "\"", "\n", "\n", "a", "//", "'", "\\", " ", "é", "x = 1;"];
    let resyncs = fuzz(&highlighter(), &SNIPPETS, 1..=40, 120);
    // The scenario must actually exercise the resynchronisation path, not only plain re-lexing.
    assert!(resyncs > 0, "fuzzing never triggered a resync");
}

#[test]
fn test_incremental_cache_equals_a_full_parse_with_embedded_languages() {
    // Deep stacks (markdown > rust > fn_body > string > interpolation > ...) must resynchronise
    // exactly like flat states do.
    const SNIPPETS: [&str; 20] = [
        "```rust\n", "```\n", "```json\n", "```\n", "fn f() {", "}", "/*", "*/", "\"", "${", "\n", "\n", "let x = 1;", "{", "[",
        "\"k\":", "# h\n", "é", " ", "x(",
    ];
    let h = HighlightPlugin::new()
        .with_language("markdown", MARKDOWN, None)
        .with_language("rust", RUST, None)
        .with_language("json", common::JSON, None);
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
fn test_render_colors_lines_inside_a_multiline_comment() {
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
fn test_render_only_parses_what_is_visible() {
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
fn test_render_wrapped_rows_share_the_logical_line_state() {
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
fn test_render_after_undo_uses_the_restored_states() {
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
fn test_view_state_visible_lines_and_syntax_ready_helpers() {
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
fn test_fast_unwrapped_viewport_render_matches_expected_content() {
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
fn test_render_colors_embedded_code_blocks() {
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
