//! Category 1: the state machine (`push`, `pop`, `set`) keeps context across lines without an AST.

mod common;

use common::*;

fn json() -> (texera::LanguageRegistry, texera::LangId) {
    registry(&[("json", JSON)])
}

fn rust() -> (texera::LanguageRegistry, texera::LangId) {
    registry(&[("rust", RUST)])
}

#[test]
fn test_set_chain_json_key_colon_value_across_lines() {
    let (reg, lang) = json();
    let lines = ["{", "", "  \"llave\"", "", "  :", "", "  \"valor\"", "  ,", "  \"otra\"", "  : 7 }"];
    let out = tokens(&reg, lang, &lines);

    assert_eq!(out[0], vec![tok("{", "punctuation.bracket")]);
    assert!(out[1].is_empty());
    assert_eq!(out[2], vec![tok("\"llave\"", "property")]);
    assert_eq!(out[4], vec![tok(":", "punctuation.delimiter")]);
    assert_eq!(out[6], vec![tok("\"valor\"", "string")]);
    assert_eq!(out[7], vec![tok(",", "punctuation.delimiter")]);
    // After the `,` the next string is a key again.
    assert_eq!(out[8], vec![tok("\"otra\"", "property")]);
    assert_eq!(out[9], vec![
        tok(":", "punctuation.delimiter"),
        tok("7", "number"),
        tok("}", "punctuation.bracket"),
    ]);
}

#[test]
fn test_set_chain_states_are_visible_in_the_stack() {
    let (reg, lang) = json();
    let states: Vec<_> = lex(&reg, lang, &["{", "\"k\"", ":", "1", "}"])
        .into_iter()
        .map(|(_, s)| reg.describe(s, lang))
        .collect();

    assert_eq!(states[0], ["json::root", "json::object_expect_key"]);
    assert_eq!(states[1], ["json::root", "json::object_expect_colon"]);
    assert_eq!(states[2], ["json::root", "json::object_expect_value"]);
    assert_eq!(states[3], ["json::root", "json::object_expect_value"]);
    assert_eq!(states[4], ["json::root"]);
}

#[test]
fn test_nested_json_objects_and_arrays_restore_the_parent_state() {
    let (reg, lang) = json();
    let line = r#"{"a": {"b": [1, "x", {"c": null}]}, "d": true, "e": "s"}"#;
    let toks = &tokens(&reg, lang, &[line])[0];

    let props: Vec<_> = toks.iter().filter(|(_, k)| k == "property").map(|(t, _)| t.as_str()).collect();
    assert_eq!(props, ["\"a\"", "\"b\"", "\"c\"", "\"d\"", "\"e\""]);
    assert!(has(toks, "\"x\"", "string"));
    assert!(has(toks, "\"s\"", "string"));
    assert!(has(toks, "null", "constant.builtin"));
    assert!(has(toks, "true", "boolean"));
    assert!(has(toks, "1", "number"));

    // Everything closed: back to the base state.
    let (_, state) = lex(&reg, lang, &[line]).pop().unwrap();
    assert!(state.is_base());
}

#[test]
fn test_contextual_keyword_let_only_inside_function_bodies() {
    let (reg, lang) = rust();
    let line = "let x = 1; fn main() { let y = 2; } let z = 3;";
    let toks = &tokens(&reg, lang, &[line])[0];

    // Exactly one `let` is a keyword: the one inside `fn_body`.
    let lets: Vec<_> = toks.iter().filter(|(t, _)| t == "let").collect();
    assert_eq!(lets.len(), 1, "root level `let`s must not be tagged: {toks:?}");
    assert_eq!(lets[0].1, "keyword.declaration");

    // And its position is the second one (inside the braces).
    let ranges = ranged(&reg, lang, line);
    let (range, _) = ranges.iter().find(|(_, k)| k == "keyword.declaration").unwrap();
    assert_eq!(&line[range.clone()], "let");
    assert_eq!(range.start, line.find("let y").unwrap());

    assert!(has(toks, "fn", "keyword.function"));
    assert!(has(toks, "main", "function.definition"));
}

#[test]
fn test_function_body_nesting_keeps_let_valid_in_inner_blocks() {
    let (reg, lang) = rust();
    let out = lex(&reg, lang, &["fn f() {", "  if true { let a = 1; }", "  let b = 2;", "}", "let c = 3;"]);
    assert!(has(&out[1].0, "let", "keyword.declaration"));
    assert!(has(&out[2].0, "let", "keyword.declaration"));
    assert!(!out[4].0.iter().any(|(_, k)| k == "keyword.declaration"));
    assert!(out[4].1.is_base());
    assert_eq!(reg.describe(out[1].1, lang), ["rust::root", "rust::fn_body"]);
}

#[test]
fn test_nested_block_comments_need_one_pop_per_closing() {
    let (reg, lang) = rust();
    let lines = ["fn f() {", "/* a /* b */ still comment", "*/ let x = 1;", "}"];
    let out = lex(&reg, lang, &lines);

    // The first `*/` makes a single pop: still inside the outer comment.
    assert_eq!(reg.describe(out[1].1, lang), ["rust::root", "rust::fn_body", "rust::block_comment"]);
    assert!(has(&out[1].0, "/* a /* b */ still comment", "comment"), "{:?}", out[1].0);
    // Only the second `*/` returns to the code, where `let` is a keyword again.
    assert_eq!(out[2].0[0], tok("*/", "comment"));
    assert!(has(&out[2].0, "let", "keyword.declaration"));
    assert_eq!(reg.describe(out[2].1, lang), ["rust::root", "rust::fn_body"]);
}

#[test]
fn test_nested_block_comment_on_a_single_line() {
    let (reg, lang) = rust();
    let line = "/* nivel 1 /* nivel 2 */ sigue siendo comentario */ fn x() { let y = 1; }";
    let toks = &tokens(&reg, lang, &[line])[0];
    assert_eq!(toks[0], tok("/* nivel 1 /* nivel 2 */ sigue siendo comentario */", "comment"));
    assert!(has(toks, "fn", "keyword.function"));
    assert!(has(toks, "let", "keyword.declaration"));
}

#[test]
fn test_string_interpolation_round_trip() {
    let (reg, lang) = rust();
    let lines = ["fn f() {", r#""hola ${user.name("admin")} mundo""#, "}"];
    let out = lex(&reg, lang, &lines);

    assert_eq!(out[1].0, vec![
        tok("\"hola ", "string"),
        tok("${", "punctuation.special"),
        tok("user", "variable"),
        tok(".", "punctuation.delimiter"),
        tok("name", "function.call"),
        tok("(", "punctuation.bracket"),
        // A second string level opened inside the interpolation.
        tok("\"admin\"", "string"),
        tok(")", "punctuation.bracket"),
        tok("}", "punctuation.special"),
        tok(" mundo\"", "string"),
    ]);
    assert_eq!(reg.describe(out[1].1, lang), ["rust::root", "rust::fn_body"]);
}

#[test]
fn test_interpolation_context_survives_the_line_break() {
    let (reg, lang) = rust();
    let lines = ["fn f() {", "\"abc ${compute(", "  1, \"x\")} tail\"", "}"];
    let out = lex(&reg, lang, &lines);

    // The line ends inside `${ ... }`: the string body below it is *not* popped (only the top state
    // is looked at) and neither is the interpolation.
    assert_eq!(
        reg.describe(out[1].1, lang),
        ["rust::root", "rust::fn_body", "rust::string_body", "rust::interpolation"]
    );
    assert!(has(&out[2].0, "1", "number"));
    assert!(has(&out[2].0, "\"x\"", "string"));
    assert!(has(&out[2].0, "}", "punctuation.special"));
    assert!(has(&out[2].0, " tail\"", "string"));
    assert_eq!(reg.describe(out[2].1, lang), ["rust::root", "rust::fn_body"]);
}

#[test]
fn test_pop_on_newline_closes_unterminated_strings() {
    let (reg, lang) = rust();
    let lines = ["fn f() {", "\"hola mundo", "let x = 10;"];
    let out = lex(&reg, lang, &lines);

    // The unclosed string does not leak into the next line.
    assert_eq!(out[1].0, vec![tok("\"hola mundo", "string")]);
    assert_eq!(out[1].1, out[0].1, "string_body was popped at the end of the line");
    assert!(has(&out[2].0, "let", "keyword.declaration"));
    assert!(has(&out[2].0, "x", "variable"));
    assert!(has(&out[2].0, "10", "number"));
    assert!(!out[2].0.iter().any(|(_, k)| k == "string"));
}

#[test]
fn test_pop_on_newline_pops_every_flagged_state_on_top() {
    let src = r#"
[states.root]
rules = [ { match = 'a', token = "a", push = "x" } ]
[states.x]
pop_on_newline = true
rules = [ { match = 'b', token = "b", push = "y" } ]
[states.y]
pop_on_newline = true
rules = [ { match = 'c', token = "c", push = "z" } ]
[states.z]
rules = [ { match = 'd', token = "d" } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    // x and y are flagged: both disappear; z is not flagged and survives together with its parents.
    let out = lex(&reg, lang, &["ab", "abc", "abcd"]);
    assert!(out[0].1.is_base());
    assert_eq!(reg.describe(out[1].1, lang), ["t::root", "t::x", "t::y", "t::z"]);
    assert_eq!(reg.describe(out[2].1, lang), ["t::root", "t::x", "t::y", "t::z"]);
}

#[test]
fn test_scan_line_equals_tokenizer_exit_state() {
    let (reg, lang) = rust();
    let lines = [
        "fn f() {", "/* open", "/* nested", "*/ back */ out", "\"str ${x(", "1)}\"", "let a = \"b\";", "}", "//done",
        "fn g(a: u32) -> u64 {", "}",
    ];
    let mut a = texera::LexState::BASE;
    let mut b = texera::LexState::BASE;
    for line in lines {
        reg.tokenize_line_stateful(lang, line, &mut a);
        b = reg.scan_line(lang, line, b);
        assert_eq!(a, b, "diverged on {line:?}");
    }
}

#[test]
fn test_lex_state_is_a_cheap_handle_and_equal_stacks_share_it() {
    assert_eq!(std::mem::size_of::<texera::LexState>(), 4);
    let (reg, lang) = rust();
    let a = lex(&reg, lang, &["fn f() {", "/*"]);
    let b = lex(&reg, lang, &["fn g() {", "/* different text"]);
    assert_eq!(a[1].1, b[1].1, "same stack, same handle");
    assert!(!a[1].1.is_base());
}
