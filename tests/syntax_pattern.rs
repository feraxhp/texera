//! Category 2: TOML compilation, patterns, `lookahead`, `include` and `default_token`.

mod common;

use common::*;
use texera::{LanguageRegistry, LexState};

fn compile_err(src: &str) -> String {
    LanguageRegistry::new()
        .register("t", src)
        .expect_err("the definition must be rejected")
        .to_string()
}

const CALLS: &str = r#"
[states.root]
rules = [
    { match = '[a-zA-Z_]\w*', lookahead = '^\s*\(', token = "function.call" },
    { match = '[a-zA-Z_]\w*', token = "variable" },
    { match = '\(', token = "punctuation.bracket" },
    { match = '\d+', token = "number" },
]
"#;

#[test]
fn test_lookahead_is_not_consumed() {
    let (reg, lang) = registry(&[("calls", CALLS)]);
    let ranges = ranged(&reg, lang, "mi_funcion(123)");

    // `mi_funcion` is 10 bytes: the call token stops right before `(`.
    assert_eq!(ranges[0], (0..10, "function.call".to_string()));
    // The cursor is exactly at byte 10: the parenthesis gets its own rule.
    assert_eq!(ranges[1], (10..11, "punctuation.bracket".to_string()));
    assert_eq!(ranges[2], (11..14, "number".to_string()));
    assert_eq!(ranges.len(), 3, "`)` has no rule and stays untagged");
}

#[test]
fn test_lookahead_tie_break_in_a_single_pass() {
    let (reg, lang) = registry(&[("calls", CALLS)]);
    let toks = &tokens(&reg, lang, &["foo   (1) + bar"])[0];

    assert_eq!(token_of(toks, "foo"), Some("function.call"));
    assert_eq!(token_of(toks, "bar"), Some("variable"));
    assert!(has(toks, "(", "punctuation.bracket"));
}

#[test]
fn test_lookahead_ending_the_line_falls_through_to_the_next_rule() {
    let (reg, lang) = registry(&[("calls", CALLS)]);
    // No `(` ever follows: plain variables.
    let toks = &tokens(&reg, lang, &["a b", "c"])[0];
    assert_eq!(toks, &vec![tok("a", "variable"), tok("b", "variable")]);
}

#[test]
fn test_lookahead_uses_the_exact_match_end_with_ambiguous_patterns() {
    // `a+` followed by `a*b`: the end of `match` must be where the *combined* automaton put it.
    let src = r#"
[states.root]
rules = [
    { match = 'a+', lookahead = 'a*b', token = "head" },
    { match = '.', token = "other" },
]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    // Leftmost-first: `a+` is greedy and takes everything it can while `a*b` still matches.
    let ranges = ranged(&reg, lang, "aaab");
    assert_eq!(ranges[0], (0..3, "head".to_string()));
    assert_eq!(ranges[1], (3..4, "other".to_string()));
}

#[test]
fn test_lookahead_leading_caret_is_relative_to_the_match_end() {
    let src_caret = r#"
[states.root]
rules = [ { match = 'x', lookahead = '^y', token = "xy" }, { match = '.', token = "any" } ]
"#;
    let src_plain = src_caret.replace("'^y'", "'y'");
    for src in [src_caret.to_string(), src_plain] {
        let (reg, lang) = registry(&[("t", &src)]);
        // `^y` must work in the *middle* of a line.
        let toks = &tokens(&reg, lang, &["zxy"])[0];
        assert_eq!(toks, &vec![tok("z", "any"), tok("x", "xy"), tok("y", "any")]);
    }
}

#[test]
fn test_strict_rule_precedence() {
    let src = r#"
[states.root]
rules = [
    { match = '\btrue\b', token = "boolean" },
    { match = '[a-z]+', token = "variable" },
]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    let toks = &tokens(&reg, lang, &["true trueish"])[0];
    assert_eq!(toks, &vec![tok("true", "boolean"), tok("trueish", "variable")]);
}

#[test]
fn test_first_matching_rule_wins_even_if_a_later_one_is_longer() {
    let src = r#"
[states.root]
rules = [
    { match = '=', token = "assign" },
    { match = '==', token = "eq" },
]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    // Two `=` (adjacent tokens with the same name are merged), never one `==`.
    let toks = &tokens(&reg, lang, &["=="])[0];
    assert_eq!(toks, &vec![tok("==", "assign")]);
    assert!(!toks.iter().any(|(_, k)| k == "eq"));
}

#[test]
fn test_anchors_refer_to_the_start_of_the_line_not_the_cursor() {
    let src = r#"
[states.root]
rules = [
    { match = '^#+', token = "heading" },
    { match = '#', token = "hash" },
    { match = '\w+', token = "word" },
]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    let toks = &tokens(&reg, lang, &["## a # b"])[0];
    assert_eq!(toks, &vec![
        tok("##", "heading"), tok("a", "word"), tok("#", "hash"), tok("b", "word"),
    ]);
}

#[test]
fn test_include_cycles_are_rejected_cleanly() {
    let mutual = r#"
[states.root]
include = ["a"]
[states.a]
include = ["b"]
[states.b]
include = ["a"]
"#;
    let err = compile_err(mutual);
    assert!(err.contains("cycle"), "{err}");
    assert!(err.contains("a -> b -> a"), "{err}");

    let own = r#"
[states.root]
include = ["root"]
"#;
    assert!(compile_err(own).contains("cycle"));

    let inline = r#"
[states.root]
rules = [ { include = "root" } ]
"#;
    assert!(compile_err(inline).contains("cycle"));
}

#[test]
fn test_include_of_unknown_state_and_unknown_transition_targets_are_errors() {
    let err = compile_err("[states.root]\ninclude = [\"nope\"]\n");
    assert!(err.contains("nope"), "{err}");
    let err = compile_err("[states.root]\nrules = [ { match = 'a', push = \"nope\" } ]\n");
    assert!(err.contains("unknown state `nope`"), "{err}");
}

#[test]
fn test_diamond_includes_are_not_cycles() {
    let src = r#"
[states.root]
include = ["left", "right"]
[states.left]
include = ["base"]
[states.right]
include = ["base"]
[states.base]
rules = [ { match = 'x', token = "x" } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    assert_eq!(tokens(&reg, lang, &["xx"])[0], vec![tok("xx", "x")]);
}

#[test]
fn test_include_position_follows_the_document_order() {
    // `include` written BEFORE `rules`: the included rules come first and win.
    let before = r#"
[states.root]
include = ["common"]
rules = [ { match = 'a', token = "own" } ]
[states.common]
rules = [ { match = 'a', token = "common" } ]
"#;
    // `include` written AFTER `rules`: the state's own rules come first.
    let after = r#"
[states.root]
rules = [ { match = 'a', token = "own" } ]
include = ["common"]
[states.common]
rules = [ { match = 'a', token = "common" }, { match = 'b', token = "common" } ]
"#;
    // An include placed inside `rules` is spliced exactly where it is written.
    let middle = r#"
[states.root]
rules = [
    { match = 'b', token = "first" },
    { include = "common" },
    { match = 'a', token = "last" },
]
[states.common]
rules = [ { match = 'a', token = "common" } ]
"#;
    let (reg, lang) = registry(&[("t", before)]);
    assert_eq!(tokens(&reg, lang, &["a"])[0], vec![tok("a", "common")]);
    let (reg, lang) = registry(&[("t", after)]);
    assert_eq!(tokens(&reg, lang, &["ab"])[0], vec![tok("a", "own"), tok("b", "common")]);
    let (reg, lang) = registry(&[("t", middle)]);
    assert_eq!(tokens(&reg, lang, &["ba"])[0], vec![tok("b", "first"), tok("a", "common")]);
}

#[test]
fn test_included_rules_keep_their_transitions() {
    let src = r#"
[states.root]
include = ["open"]
[states.open]
rules = [ { match = '\(', token = "o", push = "inner" } ]
[states.inner]
rules = [ { match = '\)', token = "c", pop = true } ]
include = ["open"]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    let out = lex(&reg, lang, &["((", "))"]);
    assert_eq!(reg.describe(out[0].1, lang), ["t::root", "t::inner", "t::inner"]);
    assert!(out[1].1.is_base());
}

#[test]
fn test_default_token_runs_are_merged_into_one_range() {
    let src = r#"
[states.root]
rules = [ { match = '/\*', token = "comment", push = "block" } ]
[states.block]
default_token = "comment"
rules = [ { match = '\*/', token = "comment", pop = true } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    // One range for the whole comment, not one per character.
    assert_eq!(ranged(&reg, lang, "/* hola mundo */"), vec![(0..16, "comment".to_string())]);
    // Multi-byte text inside the default run stays on char boundaries.
    let line = "/* ñandú 日本 */";
    assert_eq!(ranged(&reg, lang, line), vec![(0..line.len(), "comment".to_string())]);
}

#[test]
fn test_default_token_run_stops_where_a_rule_matches() {
    let src = r#"
[states.root]
default_token = "text"
rules = [ { match = '\d+', token = "num" } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    assert_eq!(ranged(&reg, lang, "ab12cd"), vec![
        (0..2, "text".to_string()),
        (2..4, "num".to_string()),
        (4..6, "text".to_string()),
    ]);
}

#[test]
fn test_text_without_default_token_is_left_untagged() {
    let (reg, lang) = registry(&[("calls", CALLS)]);
    assert_eq!(tokens(&reg, lang, &["a ; b"])[0], vec![tok("a", "variable"), tok("b", "variable")]);
}

#[test]
fn test_tokens_without_token_name_still_apply_their_transition() {
    let src = r#"
[states.root]
rules = [ { match = '"', push = "s" }, { match = 'x', token = "x" } ]
[states.s]
default_token = "string"
rules = [ { match = '"', pop = true } ]
"#;
    let (reg, lang) = registry(&[("t", src)]);
    let toks = &tokens(&reg, lang, &["x\"abc\"x"])[0];
    // The quotes are consumed by silent rules; only the inside is tagged.
    assert_eq!(toks, &vec![tok("x", "x"), tok("abc", "string"), tok("x", "x")]);
}

#[test]
fn test_definition_validation_errors_are_descriptive() {
    let err = compile_err("[states.root]\nrules = [ { token = \"x\" } ]\n");
    assert!(err.contains("missing `match`") && err.contains("root"), "{err}");

    let err = compile_err("[states.root]\nrules = [ { match = '(', token = \"x\" } ]\n");
    assert!(err.contains("invalid `match` regex"), "{err}");

    let err = compile_err("[states.root]\nrules = [ { match = 'a', push = \"root\", pop = true } ]\n");
    assert!(err.contains("mutually exclusive"), "{err}");

    let err = compile_err("[states.root]\nrules = [ { match = 'a', embed = \"x\" } ]\n");
    assert!(err.contains("embed_end"), "{err}");

    let err = compile_err("[states.root]\nrules = [ { match = 'a', embed_end = \"x\" } ]\n");
    assert!(err.contains("requires `embed`"), "{err}");

    let err = compile_err("[states.other]\nrules = []\n");
    assert!(err.contains("root"), "{err}");

    let err = compile_err("[states.root]\nrulez = []\n");
    assert!(err.contains("rulez"), "{err}");

    let err = compile_err("[states.root]\nrules = [ { match = 'a', tokken = \"x\" } ]\n");
    assert!(err.contains("tokken"), "{err}");

    assert!(compile_err("not toml at all [").contains("invalid TOML"));
    assert!(compile_err("").contains("states"));
}

#[test]
fn test_meta_extensions_resolve_languages() {
    let reg = LanguageRegistry::new();
    let id = reg.register("rust", RUST).unwrap().lang_id;
    assert_eq!(reg.lang_id("rs"), Some(id));
    assert_eq!(reg.lang_id("RUST"), Some(id));
    assert_eq!(reg.lang_id("py"), None);
    let lang = reg.language(id).unwrap();
    assert_eq!(lang.name(), "rust");
    assert_eq!(lang.extensions(), ["rs"]);
}

#[test]
fn test_shipped_syntaxes_compile_without_warnings() {
    for (name, src) in [("json", JSON), ("rust", RUST), ("markdown", MARKDOWN)] {
        let reg = LanguageRegistry::new();
        let id = reg.register(name, src).unwrap().lang_id;
        assert_eq!(reg.language(id).unwrap().warnings(), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn test_first_line_true_shebang_exclusive_to_line_zero() {
    let (reg, lang) = registry(&[("rust", RUST)]);
    let lines = [
        "#!/usr/bin/env rust-script",
        "fn main() {}",
        "#!/usr/bin/env rust-script",
    ];
    let res = lex(&reg, lang, &lines);
    // Line 0: shebang matches keyword.directive
    assert_eq!(token_of(&res[0].0, "#!/usr/bin/env rust-script"), Some("keyword.directive"));
    // Line 1: fn keyword matches keyword.function
    assert_eq!(token_of(&res[1].0, "fn"), Some("keyword.function"));
    // Line 2: shebang does NOT match keyword.directive because first_line rule only exists in doc_start
    assert_ne!(token_of(&res[2].0, "#!/usr/bin/env rust-script"), Some("keyword.directive"));
}

#[test]
fn test_first_line_true_markdown_frontmatter() {
    let (reg, lang) = registry(&[("markdown", MARKDOWN)]);

    // Document with frontmatter on line 0
    let lines_with_fm = [
        "---",
        "title: hello",
        "---",
        "# Heading",
    ];
    let res_fm = lex(&reg, lang, &lines_with_fm);
    assert_eq!(token_of(&res_fm[0].0, "---"), Some("punctuation.delimiter"), "line 0 opens yaml_frontmatter");
    assert_eq!(token_of(&res_fm[1].0, "title"), Some("attribute"), "line 1 attribute inside yaml_frontmatter");
    assert_eq!(token_of(&res_fm[2].0, "---"), Some("punctuation.delimiter"), "line 2 closes yaml_frontmatter");
    assert_eq!(token_of(&res_fm[3].0, "# Heading"), Some("markup.heading"));

    // Document with --- starting on line 2 (not frontmatter, but horizontal rule)
    let lines_no_fm = [
        "# Heading",
        "Some paragraph",
        "---",
        "title: hello",
    ];
    let res_no_fm = lex(&reg, lang, &lines_no_fm);
    assert_eq!(token_of(&res_no_fm[2].0, "---"), Some("punctuation.special"), "line 2 is horizontal rule, not frontmatter");
    assert_ne!(token_of(&res_no_fm[3].0, "title"), Some("attribute"), "line 3 is not inside yaml_frontmatter");
}

#[test]
fn test_first_line_degradation_with_nested_push_on_line_zero() {
    let (reg, lang) = registry(&[("rust", RUST)]);
    // Line 0 opens a function (pushing fn_signature and then fn_body)
    let lines = [
        "fn main() {",
        "    let x = 1;",
        "}",
        "#!/usr/bin/env rust-script",
    ];
    let res = lex(&reg, lang, &lines);
    // Line 1: let is inside fn_body
    assert_eq!(token_of(&res[1].0, "let"), Some("keyword.declaration"));
    // Line 2: } closes fn_body and pops back to base frame
    // The base frame degraded to ROOT on line 0, so after popping, state is ROOT, not DOC_START!
    assert_eq!(res[2].1, LexState::BASE);
    // Line 3: shebang must NOT be keyword.directive
    assert_ne!(token_of(&res[3].0, "#!/usr/bin/env rust-script"), Some("keyword.directive"));
}

#[test]
fn test_first_line_validation_and_reserved_doc_start() {
    let compile_err = |src: &str| texera::plugins::highlight::lang::compile(src).unwrap_err().to_string();

    // User cannot define [states.doc_start]
    let doc_start_reserved = r#"
[states.root]
rules = []
[states.doc_start]
rules = []
"#;
    assert!(compile_err(doc_start_reserved).contains("reserved"));

    // User cannot transition to doc_start
    let push_doc_start = r#"
[states.root]
rules = [{ match = "a", push = "doc_start" }]
"#;
    assert!(compile_err(push_doc_start).contains("doc_start"));

    // User cannot use first_line outside root
    let first_line_in_other = r#"
[states.root]
rules = []
[states.other]
rules = [{ match = "a", first_line = true }]
"#;
    assert!(compile_err(first_line_in_other).contains("only allowed in `root`"));

    // User cannot combine include with first_line
    let include_first_line = r#"
[states.root]
rules = [{ include = "other", first_line = true }]
[states.other]
rules = []
"#;
    assert!(compile_err(include_first_line).contains("include"));
}

#[test]
fn test_highlight_line_respects_line_index_for_doc_start() {
    use texera::{Highlight, HighlightPlugin, ThemeMap};
    let mut theme = ThemeMap::default();
    theme.set_color("keyword.directive", ratatui::style::Color::Yellow);
    let h = HighlightPlugin::new().with_language("rust", RUST, Some(theme));
    let shebang = "#!/usr/bin/env rust-script";

    let line0 = h.highlight_line(shebang, 0);
    let line1 = h.highlight_line(shebang, 1);

    assert!(!line0.is_empty(), "line 0 should have directive token");
    assert!(line1.is_empty(), "line 1 should have no tokens in root");
}
