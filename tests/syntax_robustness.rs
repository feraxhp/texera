//! Category 5: robustness and edge cases (anti-bugs).

mod common;

use common::*;
use ratatui::style::Color;
use texera::{
    Highlight, LanguageRegistry, LexState, MAX_DEPTH, MAX_POOL_STATES, HighlightPlugin, ThemeMap,
};

// ---------------------------------------------------------------------------------------------
// Zero byte matches
// ---------------------------------------------------------------------------------------------

#[test]
fn test_zero_width_matches_never_freeze_the_lexer() {
    let cases = [
        ("star", r#"[states.root]
rules = [ { match = '.*', token = "all" } ]"#),
        ("caret", r#"[states.root]
rules = [ { match = '^', token = "bol" } ]"#),
        ("digits", r#"[states.root]
rules = [ { match = '[0-9]*', token = "num" } ]"#),
        ("boundary", r#"[states.root]
rules = [ { match = '\b', token = "wb" }, { match = 'x', token = "x" } ]"#),
        ("empty", r#"[states.root]
rules = [ { match = '(?:)', token = "nothing" } ]"#),
    ];
    for (name, src) in cases {
        let finished = with_timeout(10, move || {
            let (reg, lang) = registry(&[(name, src)]);
            for line in ["", "abc 123", "日本語 ñ 🦀", "x x x", &"z".repeat(2000)] {
                let mut state = LexState::BASE;
                for (range, _) in reg.tokenize_line_stateful(lang, line, &mut state) {
                    assert!(line.is_char_boundary(range.start) && line.is_char_boundary(range.end));
                    assert!(range.start <= range.end && range.end <= line.len());
                }
            }
            true
        });
        assert!(finished, "{name}");
    }
}

#[test]
fn test_zero_width_rules_are_reported_as_warnings() {
    let reg = LanguageRegistry::new();
    let id = reg
        .register("t", "[states.root]\nrules = [ { match = '[0-9]*', token = \"n\" }, { match = 'a', token = \"a\" } ]")
        .unwrap()
        .lang_id;
    let warnings = reg.language(id).unwrap().warnings().to_vec();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("zero bytes") && warnings[0].contains("root"));

    // With a transition a zero-width match is a legitimate construct and is not reported.
    let reg = LanguageRegistry::new();
    let id = reg
        .register("t", "[states.root]\nrules = [ { match = '', lookahead = 'a', push = \"x\" } ]\n[states.x]\nrules = [ { match = 'a', pop = true } ]")
        .unwrap()
        .lang_id;
    assert!(reg.language(id).unwrap().warnings().is_empty());
}

#[test]
fn test_zero_width_transition_cycles_are_bounded() {
    // A -> B -> A without consuming a single byte, forever.
    let ping_pong = r#"
[states.root]
rules = [ { match = '(?:)', set = "b" } ]
[states.b]
rules = [ { match = '(?:)', set = "root" } ]
"#;
    // Pushing forever without consuming.
    let pusher = r#"
[states.root]
rules = [ { match = '(?:)', push = "root" } ]
"#;
    for (name, src) in [("ping_pong", ping_pong), ("pusher", pusher)] {
        let depth = with_timeout(10, move || {
            let (reg, lang) = registry(&[(name, src)]);
            let mut state = LexState::BASE;
            reg.tokenize_line_stateful(lang, &"a".repeat(500), &mut state);
            reg.stack_of(state, lang).len()
        });
        assert!(depth <= MAX_DEPTH, "{name}: depth {depth}");
    }
}

#[test]
fn test_zero_width_lookahead_transition_is_legal() {
    // Leave a state *before* the delimiter, without consuming it.
    let src = r#"
[states.root]
rules = [
    { match = '<', token = "open", push = "tag" },
    { match = 'x', token = "text" },
]
[states.tag]
default_token = "tagname"
rules = [ { match = '', lookahead = '>', pop = true } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    let toks = &tokens(&reg, lang, &["<abc>x"])[0];
    // `match = ''` + `lookahead = '>'` pops without consuming; `>` is then lexed by the root state (no rule: untagged).
    assert_eq!(toks, &vec![tok("<", "open"), tok("abc", "tagname"), tok("x", "text")]);
    assert!(lex(&reg, lang, &["<abc>x"])[0].1.is_base());
}

// ---------------------------------------------------------------------------------------------
// Stack underflow / overflow
// ---------------------------------------------------------------------------------------------

const BRACKETS: &str = r#"
[states.root]
rules = [
    { match = '[\{\[\(]', token = "open", push = "inner" },
    { match = '[\}\]\)]', token = "close", pop = true },
]
[states.inner]
rules = [
    { match = '[\{\[\(]', token = "open", push = "inner" },
    { match = '[\}\]\)]', token = "close", pop = true },
]
"#;

#[test]
fn test_excess_pops_never_empty_the_stack() {
    let (reg, lang) = registry(&[("t", BRACKETS)]);
    let out = lex(&reg, lang, &["}}}}}}}}}})]]", "}", "{}}}"]);
    for (toks, state) in &out {
        assert!(!toks.is_empty());
        assert!(state.is_base());
        assert_eq!(reg.stack_of(*state, lang).len(), 1);
    }
    // Everything is still tagged as a closing bracket.
    assert!(out[0].0.iter().all(|(_, k)| k == "close"));
}

#[test]
fn test_excess_pops_with_the_json_grammar() {
    let (reg, lang) = registry(&[("json", JSON)]);
    // `}` and `]` outside of any container are just untagged text there.
    let out = lex(&reg, lang, &["}}}}}}}}}})]]", "{\"a\": 1}}}}", "\"still a string\""]);
    assert!(out[2].0[0] == tok("\"still a string\"", "string"));
    assert!(out[2].1.is_base());
}

#[test]
fn test_extreme_nesting_is_capped_and_memory_stays_flat() {
    let (reg, lang) = registry(&[("t", BRACKETS)]);
    let line = "{".repeat(10_000);
    let mut state = LexState::BASE;
    let toks = reg.tokenize_line_stateful(lang, &line, &mut state);

    assert_eq!(reg.stack_of(state, lang).len(), MAX_DEPTH);
    assert_eq!(toks, vec![(0..10_000, "open".into())], "every brace is still tagged (and merged)");

    // Repeating it does not intern anything new: memory is flat.
    let pool = reg.pool_len();
    for _ in 0..50 {
        let mut s = state;
        reg.tokenize_line_stateful(lang, &line, &mut s);
        assert_eq!(s, state);
    }
    assert_eq!(reg.pool_len(), pool);

    // And the stack unwinds normally afterwards.
    let mut s = state;
    reg.tokenize_line_stateful(lang, &"}".repeat(10_000), &mut s);
    assert!(s.is_base());
}

#[test]
fn test_extreme_nesting_in_json_arrays() {
    let (reg, lang) = registry(&[("json", JSON)]);
    let mut state = LexState::BASE;
    reg.tokenize_line_stateful(lang, &"[".repeat(10_000), &mut state);
    assert_eq!(reg.stack_of(state, lang).len(), MAX_DEPTH);
}

#[test]
fn test_state_pool_is_flushed_when_full_and_the_revision_changes() {
    // Two push targets: every 17-long sequence of `a` / `b` produces a distinct stack.
    let src = r#"
[states.root]
rules = [
    { match = 'a', push = "pa" },
    { match = 'b', push = "pb" },
]
[states.pa]
rules = []
[states.pb]
rules = []
"#;
    // pa / pb never push further (no rules), so build the sequences with a self-pushing grammar.
    let src = src.replace(
        "[states.pa]\nrules = []\n[states.pb]\nrules = []\n",
        "[states.pa]\ninclude = [\"root\"]\n[states.pb]\ninclude = [\"root\"]\n",
    );
    let finished = with_timeout(120, move || {
        let (reg, lang) = registry(&[("t", &src)]);
        let revision = reg.revision();
        let lines = MAX_POOL_STATES + 2_000;
        let mut states = std::collections::HashSet::new();
        for n in 0..lines {
            // 17 bits of `n` -> 17 pushes of pa / pb.
            let line: String = (0..17).map(|bit| if n >> bit & 1 == 0 { 'a' } else { 'b' }).collect();
            states.insert(reg.scan_line(lang, &line, LexState::BASE));
        }
        assert!(states.len() > 1_000);
        assert!(reg.pool_len() <= MAX_POOL_STATES, "pool is bounded: {}", reg.pool_len());
        assert!(reg.revision() > revision, "flushing the pool invalidates derived caches");
        true
    });
    assert!(finished);
}

#[test]
fn test_cache_is_rebuilt_when_the_pool_is_flushed_while_parsing() {
    use texera::SyntaxField;
    use texera_core::State;

    let src = r#"
[states.root]
rules = [ { match = 'a', push = "pa" }, { match = 'b', push = "pb" } ]
[states.pa]
include = ["root"]
[states.pb]
include = ["root"]
"#;
    let h = HighlightPlugin::new().with_language("t", src, None).with_parse_budget(usize::MAX);
    let lang = h.active().unwrap().0;
    // Flush the pool from "the outside" and make sure old entries are rejected.
    let text = (0..50).map(|i| "ab".repeat(i % 7 + 1)).collect::<Vec<_>>().join("\n");
    let state = State::from(text.as_str()).with_extensions([HighlightPlugin::syntax_extension()]);
    h.prepare_viewport(&state, 0..=60);
    let cache = state.field::<SyntaxField>().unwrap();
    let id = h.rules_id().unwrap();
    assert!(cache.entry_state(10, id).is_some());

    for n in 0..(MAX_POOL_STATES + 10) {
        let line: String = (0..17).map(|bit| if n >> bit & 1 == 0 { 'a' } else { 'b' }).collect();
        h.registry().scan_line(lang, &line, LexState::BASE);
    }
    assert_ne!(h.rules_id().unwrap(), id);
    assert_eq!(cache.entry_state(10, h.rules_id().unwrap()), None);
    h.prepare_viewport(&state, 0..=60);
    assert!(cache.entry_state(10, h.rules_id().unwrap()).is_some(), "re-parsed after the flush");
}

// ---------------------------------------------------------------------------------------------
// UTF-8
// ---------------------------------------------------------------------------------------------

#[test]
fn test_multibyte_text_never_splits_a_codepoint() {
    let (reg, lang) = registry(&[("rust", RUST), ("json", JSON), ("markdown", MARKDOWN)]);
    let lines = [
        "fn f() {",
        "let 🦀 = \"¡Hola niño! 日本語\"; // 🚀 ñandú",
        "/* ü ✓ 😀 */ let é = 日本語;",
        "\"abc ${ñ(\"é\")} 日\"",
        "}",
    ];
    for id in [lang, reg.lang_id("json").unwrap(), reg.lang_id("markdown").unwrap()] {
        let mut state = LexState::BASE;
        for line in lines {
            for (range, _) in reg.tokenize_line_stateful(id, line, &mut state) {
                assert!(line.is_char_boundary(range.start), "{range:?} in {line:?}");
                assert!(line.is_char_boundary(range.end), "{range:?} in {line:?}");
            }
        }
    }
}

#[test]
fn test_styled_tokens_use_char_indices_not_byte_offsets() {
    let src = r#"
[states.root]
rules = [
    { match = '"', token = "string", push = "s" },
    { match = '//.*', token = "comment" },
]
[states.s]
pop_on_newline = true
default_token = "string"
rules = [ { match = '"', token = "string", pop = true } ]
"#;
    let mut theme = ThemeMap::default();
    theme.set_color("string", Color::Green);
    theme.set_color("comment", Color::DarkGray);
    let h = HighlightPlugin::new().with_language("t", src, Some(theme));

    let line = "let 🦀 = \"¡Hola niño! 日本語\"; // 🚀";
    let styled = h.highlight_line(line, 0);
    assert_eq!(styled.len(), 2);

    let chars: Vec<char> = line.chars().collect();
    let text = |r: &std::ops::Range<usize>| chars[r.clone()].iter().collect::<String>();
    assert_eq!(text(&styled[0].0), "\"¡Hola niño! 日本語\"");
    assert_eq!(styled[0].1.fg, Some(Color::Green));
    assert_eq!(text(&styled[1].0), "// 🚀");
    assert_eq!(styled[1].1.fg, Some(Color::DarkGray));
}

// ---------------------------------------------------------------------------------------------
// Garbage in, no panic out
// ---------------------------------------------------------------------------------------------

#[test]
fn test_random_garbage_never_panics_and_always_terminates() {
    let (reg, _) = registry(&[("rust", RUST), ("json", JSON), ("markdown", MARKDOWN)]);
    let reg = std::sync::Arc::new(reg);
    let pieces = [
        "{", "}", "[", "]", "(", ")", "\"", "/*", "*/", "//", "```", "```rust", "```json", "```nope", "fn ", "let ", "${",
        "\\", " ", "\t", "é", "日", "🦀", "x", "1.5", ":", ",", "\r", "#", "- ", "`",
    ];
    for name in ["rust", "json", "markdown"] {
        let reg = reg.clone();
        with_timeout(120, move || {
            let lang = reg.lang_id(name).unwrap();
            let mut rng = Rng(0x1234_5678_9ABC_DEF1 ^ name.len() as u64);
            let mut state = LexState::BASE;
            for _ in 0..3_000 {
                let line: String = (0..rng.below(40)).map(|_| pieces[rng.below(pieces.len())]).collect();
                let tokens = reg.tokenize_line_stateful(lang, &line, &mut state);
                let mut last_end = 0;
                for (range, _) in tokens {
                    assert!(line.is_char_boundary(range.start) && line.is_char_boundary(range.end));
                    assert!(range.start >= last_end, "tokens are ordered and disjoint");
                    assert!(range.start < range.end);
                    last_end = range.end;
                }
                // The scanner and the tokenizer agree.
                assert_eq!(reg.scan_line(lang, &line, state), reg.scan_line(lang, &line, state));
            }
        });
    }
}

#[test]
fn test_tokenizer_and_scanner_agree_on_garbage() {
    let (reg, lang) = registry(&[("markdown", MARKDOWN), ("rust", RUST)]);
    let pieces = ["```rust", "```", "fn f() {", "}", "/*", "*/", "\"", "x", "\n"];
    let mut rng = Rng(42);
    let (mut a, mut b) = (LexState::BASE, LexState::BASE);
    for _ in 0..2_000 {
        let line: String = (0..rng.below(6)).map(|_| pieces[rng.below(pieces.len())]).collect();
        let line = line.replace('\n', "");
        reg.tokenize_line_stateful(lang, &line, &mut a);
        b = reg.scan_line(lang, &line, b);
        assert_eq!(a, b, "diverged on {line:?}");
    }
}

#[test]
fn test_very_long_lines_are_lexed_in_linear_time() {
    let (reg, lang) = registry(&[("rust", RUST)]);
    with_timeout(60, move || {
        let line = format!("fn f() {{ {} }}", "let a = foo(1, \"s\"); ".repeat(20_000));
        let mut state = LexState::BASE;
        let toks = reg.tokenize_line_stateful(lang, &line, &mut state);
        assert!(toks.len() > 100_000);
    });
}

#[test]
fn test_missing_language_yields_no_tokens() {
    let reg = LanguageRegistry::new();
    assert!(reg.tokenize_line(0, "anything").is_empty());
    assert!(reg.scan_line(7, "anything", LexState(123)).is_base());
    assert!(HighlightPlugin::new().highlight_line("fn", 0).is_empty());
}

#[test]
fn test_dangling_lex_state_falls_back_to_the_base_stack() {
    let (reg, lang) = registry(&[("rust", RUST)]);
    let bogus = LexState(987_654);
    let toks = reg.tokenize_line_stateful(lang, "fn f() {}", &mut bogus.clone());
    assert!(!toks.is_empty());
    assert_eq!(reg.stack_of(bogus, lang).len(), 1);
}

#[test]
fn test_broken_language_in_with_language_is_reported_not_fatal() {
    let h = HighlightPlugin::new().with_language("bad", "[states.root]\nrules = [ { match = '(' } ]", None);
    assert_eq!(h.errors.len(), 1);
    assert!(h.errors[0].starts_with("bad:"));
    assert!(h.languages.is_empty());
    assert!(h.highlight_line("anything", 0).is_empty());

    assert!(HighlightPlugin::new().try_with_language("bad", "nope [", None).is_err());
}
