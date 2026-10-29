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

//! Test helpers for syntax cache testing.

use texera_core::TransactionSpec;
use texera_core::ChangeSpec;
use ratatui::style::Color;
use texera_core::State;
use texera_core::Text;

use texera::HighlightPlugin;
use texera::HistoryPlugin;
use texera::SyntaxCache;
use texera::SyntaxField;
use texera::Highlight;
use texera::LexState;
use texera::ThemeMap;

/// `//` comments, nestable multi-line `/* */` comments and multi-line `"..."` strings.
pub const TEST_LANG: &str = r#"
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

pub fn highlighter() -> HighlightPlugin {
    let mut theme = ThemeMap::default();
    theme.set_color("comment", Color::DarkGray);
    theme.set_color("string", Color::Green);
    HighlightPlugin::new().with_language("test", TEST_LANG, Some(theme))
}

pub fn plain_lines(n: usize) -> String {
    (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

pub fn state_with_cache(text: &str) -> State {
    State::from(text).with_extensions([HighlightPlugin::syntax_extension(), HistoryPlugin::extension(None)])
}

pub fn cache(state: &State) -> &SyntaxCache {
    state.field::<SyntaxField>().expect("SyntaxField registered")
}

pub fn line_start(state: &State, line: usize) -> usize {
    state.doc().line(line).unwrap().from()
}

/// Replaces `from..to` (char offsets) with `insert`.
pub fn edit(state: State, from: usize, to: usize, insert: &str) -> State {
    let spec = TransactionSpec::default()
        .with_change(ChangeSpec::new(from, Some(to), Some(Text::from(insert))).unwrap());
    state.update(spec).unwrap().into_state()
}

/// Entry states of the whole document computed from scratch by a brand-new cache. The registry is
/// shared with `h`, so equal stacks have equal handles.
pub fn fresh_states(h: &HighlightPlugin, text: &str) -> Vec<LexState> {
    let h = h.clone().with_parse_budget(usize::MAX);
    let state = state_with_cache(text);
    let total = state.doc().len_lines();
    h.prepare_viewport(&state, 0..=total);
    cache(&state).known_states()
}

/// Every verified entry of the cache must equal what a from-scratch parse produces.
pub fn assert_cache_consistent(h: &HighlightPlugin, state: &State, context: &str) {
    let known = cache(state).known_states();
    let fresh = fresh_states(h, &state.doc().to_string());
    assert!(known.len() <= fresh.len(), "{context}: cache knows more lines than the doc has");
    assert_eq!(known, fresh[..known.len()], "{context}: cached states diverge from a fresh parse");
}

/// Entry state of `line` as the readable stack, e.g. `["test::root", "test::block_comment"]`.
pub fn stack_at(h: &HighlightPlugin, state: &State, line: usize) -> Option<Vec<String>> {
    let (lang, _) = h.active().unwrap();
    cache(state)
        .entry_state(line, h.rules_id().unwrap())
        .map(|s| h.registry().describe(s, lang))
}

pub fn comment_depth(h: &HighlightPlugin, state: &State, line: usize) -> Option<usize> {
    stack_at(h, state, line).map(|s| s.iter().filter(|f| f.ends_with("block_comment")).count())
}

pub fn entry(h: &HighlightPlugin, state: &State, line: usize) -> Option<LexState> {
    cache(state).entry_state(line, h.rules_id().unwrap())
}
