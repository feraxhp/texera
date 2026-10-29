//! Category 4: language injection (`embed`), on-demand loading and overriding languages.

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::Arc;

use common::MARKDOWN;
use common::registry;
use common::RUST;
use common::has;
use common::lex;
use common::tok;

use texera::HighlightPlugin;
use ratatui::style::Color;
use texera::SyntaxField;
use texera_core::State;
use texera::Highlight;
use texera::ThemeMap;

const PYTHON: &str = r#"
[meta]
name = "python"
extensions = ["py"]

[states.root]
rules = [
    { match = '\b(?:def|return)\b', token = "keyword" },
    { match = '#.*', token = "comment" },
]
"#;

fn md_with_rust() -> (texera::LanguageRegistry, texera::LangId) {
    registry(&[("markdown", MARKDOWN), ("rust", RUST)])
}

const DOC: [&str; 8] = [
    "# Title",
    "plain `code` text",
    "```rust",
    "fn main() {",
    "    let y = 2;",
    "}",
    "```",
    "after fn let",
];

#[test]
fn markdown_with_embedded_rust_full_cycle() {
    let (reg, md) = md_with_rust();
    let out = lex(&reg, md, &DOC);

    // Before the block: markdown rules.
    assert_eq!(out[0].0, vec![tok("# Title", "markup.heading")]);
    assert_eq!(out[1].0, vec![tok("`code`", "markup.raw")]);
    assert!(out[1].1.is_base());

    // Opening fence: `embed = "$1"` fires and pushes rust.
    assert_eq!(out[2].0, vec![tok("```rust", "punctuation.delimiter")]);
    assert_eq!(reg.describe(out[2].1, md), ["markdown::root", "rust::root"]);

    // Inside: rust rules, and rust context survives the line breaks.
    assert!(has(&out[3].0, "fn", "keyword.function"));
    assert!(has(&out[3].0, "main", "function.definition"));
    assert_eq!(reg.describe(out[3].1, md), ["markdown::root", "rust::root", "rust::fn_body"]);
    assert!(has(&out[4].0, "let", "keyword.declaration"));
    assert_eq!(reg.describe(out[5].1, md), ["markdown::root", "rust::root"]);

    // Closing fence: pops the embedded context and gives control back to markdown.
    assert_eq!(out[6].0, vec![tok("```", "punctuation.delimiter")]);
    assert!(out[6].1.is_base());

    // After the block: markdown again (`fn` and `let` mean nothing there).
    assert!(out[7].0.is_empty());
}

#[test]
fn embed_language_is_resolved_by_extension_and_case() {
    let (reg, md) = md_with_rust();
    for fence in ["```rs", "```Rust", "```RUST"] {
        let out = lex(&reg, md, &[fence, "fn x() {}"]);
        assert!(has(&out[1].0, "fn", "keyword.function"), "{fence}");
    }
}

#[test]
fn embed_end_token_and_fixed_embed_name() {
    let host = r#"
[states.root]
rules = [
    { match = '^<<<', token = "open", embed = "rust", embed_end = '^>>>', embed_end_token = "close" },
]
"#;
    let (reg, lang) = registry(&[("host", host), ("rust", RUST)]);
    let out = lex(&reg, lang, &["<<<", "fn a() {}", ">>> tail", "fn b() {}"]);
    assert_eq!(out[0].0, vec![tok("<<<", "open")]);
    assert!(has(&out[1].0, "fn", "keyword.function"));
    assert_eq!(out[2].0, vec![tok(">>>", "close")]);
    assert!(out[2].1.is_base());
    assert!(out[3].0.is_empty(), "back in the host language");
}

#[test]
fn fence_without_language_is_plain_text_until_the_closing_fence() {
    let (reg, md) = md_with_rust();
    let out = lex(&reg, md, &["```", "fn let `x` # not a heading", "```", "# heading"]);
    assert_eq!(reg.describe(out[0].1, md), ["markdown::root", "<plain>"]);
    assert!(out[1].0.is_empty());
    assert_eq!(out[2].0, vec![tok("```", "punctuation.delimiter")]);
    assert_eq!(out[3].0, vec![tok("# heading", "markup.heading")]);
}

#[test]
fn the_host_delimiter_always_wins() {
    // Documented limit of single pass lexers: whatever the embedded language has open (here an
    // unterminated block comment) the host's closing delimiter ends the block.
    let (reg, md) = md_with_rust();
    let out = lex(&reg, md, &["```rust", "fn f() { /* open", "still comment", "```", "text"]);
    assert_eq!(out[1].0.last().unwrap(), &tok("/* open", "comment"));
    assert_eq!(reg.describe(out[2].1, md).last().unwrap(), "rust::block_comment");
    assert_eq!(out[3].0, vec![tok("```", "punctuation.delimiter")]);
    assert!(out[3].1.is_base());
    assert!(out[4].0.is_empty());
}

#[test]
fn nested_embeds_close_innermost_first() {
    let (reg, md) = md_with_rust();
    let out = lex(&reg, md, &["```markdown", "```rust", "fn f() {", "```", "# inner heading", "```", "# outer heading"]);
    assert_eq!(reg.describe(out[1].1, md), ["markdown::root", "markdown::root", "rust::root"]);
    assert!(has(&out[2].0, "fn", "keyword.function"));
    // First closing fence: only the rust block ends, we are back in the embedded markdown.
    assert_eq!(reg.describe(out[3].1, md), ["markdown::root", "markdown::root"]);
    assert_eq!(out[4].0, vec![tok("# inner heading", "markup.heading")]);
    // Second one ends the markdown block.
    assert!(out[5].1.is_base());
    assert_eq!(out[6].0, vec![tok("# outer heading", "markup.heading")]);
}

#[test]
fn embedded_root_frame_cannot_be_popped() {
    let host = r#"
[states.root]
rules = [ { match = '^<<(\w+)', token = "open", embed = "$1", embed_end = '^>>$' } ]
"#;
    let guest = r#"
[states.root]
rules = [
    { match = '\}', token = "close", pop = true },
    { match = '\{', token = "open", push = "blk" },
]
[states.blk]
rules = [ { match = '\}', token = "close", pop = true } ]
"#;
    let (reg, lang) = registry(&[("host", host), ("inner", guest)]);
    let out = lex(&reg, lang, &["<<inner", "}}}}", "{ {", "}}}", ">>", "x"]);

    // Unbalanced `}` never leave the embedded language by themselves...
    assert_eq!(reg.describe(out[1].1, lang), ["host::root", "inner::root"]);
    assert_eq!(reg.describe(out[2].1, lang), ["host::root", "inner::root", "inner::blk"]);
    assert_eq!(reg.describe(out[3].1, lang), ["host::root", "inner::root"]);
    // ...only the host delimiter does.
    assert!(out[4].1.is_base());
}

#[test]
fn lazy_loader_is_called_once_per_language_and_cached() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = calls.clone();
    let (reg, md) = registry(&[("markdown", MARKDOWN)]);
    reg.set_loader(move |name| {
        log.lock().unwrap().push(name.to_string());
        (name == "python").then(|| PYTHON.to_string())
    });
    let revision = reg.revision();
    assert!(reg.lang_id("python").is_none(), "not loaded yet");

    let doc = [
        "```python", "def f(): return 1  # c", "```", "text def",
        "```python", "return", "```",
        "```brainfuck", "+++[->+<] def", "```",
        "```brainfuck", "+", "```",
    ];
    let out = lex(&reg, md, &doc);

    // 1. Loaded on demand, compiled and highlighted.
    assert!(has(&out[1].0, "def", "keyword"));
    assert!(has(&out[1].0, "return", "keyword"));
    assert!(has(&out[1].0, "# c", "comment"));
    assert!(reg.lang_id("python").is_some());
    // 2. The second python block reuses the compiled language.
    assert!(has(&out[5].0, "return", "keyword"));
    // 3. A language the loader does not know never panics: neutral state, no tags...
    assert_eq!(reg.describe(out[7].1, md), ["markdown::root", "<plain>"]);
    assert!(out[8].0.is_empty());
    // ...until the closing fence.
    assert!(out[9].1.is_base());
    assert!(out[3].0.is_empty());

    // The loader saw each name exactly once, failures included.
    assert_eq!(*calls.lock().unwrap(), ["python", "brainfuck"]);
    // Lazily loading a language that was not known does not invalidate cached lines.
    assert_eq!(reg.revision(), revision);
}

#[test]
fn loader_returning_garbage_is_treated_as_unavailable() {
    let (reg, md) = registry(&[("markdown", MARKDOWN)]);
    reg.set_loader(|_| Some("this is [not a valid language".to_string()));
    let out = lex(&reg, md, &["```whatever", "text", "```", "# h"]);
    assert_eq!(reg.describe(out[0].1, md), ["markdown::root", "<plain>"]);
    assert!(out[2].1.is_base());
    assert_eq!(out[3].0, vec![tok("# h", "markup.heading")]);
}

#[test]
fn late_availability_requires_a_revision_bump_to_take_effect() {
    let available = Arc::new(AtomicUsize::new(0));
    let flag = available.clone();
    let (reg, md) = registry(&[("markdown", MARKDOWN)]);
    reg.set_loader(move |_| (flag.load(Ordering::SeqCst) == 1).then(|| PYTHON.to_string()));

    let before = reg.revision();
    let out = lex(&reg, md, &["```python", "def"]);
    assert!(out[1].0.is_empty(), "not available yet");

    // The language "arrives" later (asynchronous load): same text, but the registry is told.
    available.store(1, Ordering::SeqCst);
    let revision = reg.retry_unavailable();
    assert!(revision > before);
    assert_ne!(reg.rules_id(md), (before << 16) | md as u64);

    let out = lex(&reg, md, &["```python", "def"]);
    assert!(has(&out[1].0, "def", "keyword"));
}

#[test]
fn registering_a_language_returns_the_new_revision() -> Result<(), Box<dyn std::error::Error>> {
    let (reg, _) = md_with_rust();
    let before = reg.revision();
    let registered = reg.register("rust", RUST)?;
    assert!(registered.revision > before);
    assert_eq!(registered.revision, reg.revision());
    // Overriding keeps the id so existing stacks stay meaningful.
    assert_eq!(Some(registered.lang_id), reg.lang_id("rust"));

    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Integration with the incremental cache
// ---------------------------------------------------------------------------------------------

const RUST_OVERRIDE: &str = r#"
[meta]
name = "rust"
[states.root]
rules = [ { match = '\bfn\b', token = "keyword.custom" } ]
"#;

fn highlighter() -> HighlightPlugin {
    let mut theme = ThemeMap::default();
    theme.set_color("keyword.function", Color::Magenta);
    theme.set_color("keyword.custom", Color::Cyan);
    theme.set_color("markup.heading", Color::Yellow);
    HighlightPlugin::new()
        .with_language("markdown", MARKDOWN, Some(theme.clone()))
        .with_language("rust", RUST, Some(theme))
}

fn state(text: &str) -> State {
    State::from(text).with_extensions([HighlightPlugin::syntax_extension()])
}

fn fg_at(styled: &[(std::ops::Range<usize>, ratatui::style::Style)], col: usize) -> Option<Color> {
    styled.iter().find(|(r, _)| r.contains(&col)).and_then(|(_, s)| s.fg)
}

#[test]
fn embedded_tokens_are_colored_through_the_cached_state() {
    let h = highlighter();
    let text = DOC.join("\n");
    let st = state(&text);
    h.prepare_viewport(&st, 0..=7);

    // `fn` (line 3) is inside the rust block: keyword.function -> magenta.
    let line3 = h.highlight_document_line(&st, 3, DOC[3]);
    assert_eq!(fg_at(&line3, 0), Some(Color::Magenta));
    // The heading is plain markdown.
    assert_eq!(fg_at(&h.highlight_document_line(&st, 0, DOC[0]), 0), Some(Color::Yellow));
    // Without the cached state the stateless fallback lexes line 3 as markdown: no color.
    assert_eq!(fg_at(&h.highlight_line(DOC[3], 3), 0), None);
}

#[test]
fn overriding_a_language_invalidates_the_cached_parse() {
    let h = highlighter();
    let text = DOC.join("\n");
    let st = state(&text);
    h.prepare_viewport(&st, 0..=7);

    let cache = st.field::<SyntaxField>().unwrap();
    let old_id = h.rules_id().unwrap();
    let lexed = cache.stats().lines_lexed;
    assert_eq!(fg_at(&h.highlight_document_line(&st, 3, DOC[3]), 0), Some(Color::Magenta));
    assert!(cache.entry_state(3, old_id).is_some());

    // The user loads a custom rust definition from disk (here: a string) over the embedded one.
    let revision = h.register_language("rust", RUST_OVERRIDE).unwrap();
    assert!(revision > 0);
    let new_id = h.rules_id().unwrap();
    assert_ne!(old_id, new_id);
    assert_eq!(cache.entry_state(3, new_id), None, "old entries do not belong to the new rules");

    // Until the next `advance`, painting falls back instead of using stale states.
    let _ = h.highlight_document_line(&st, 3, DOC[3]);

    h.prepare_viewport(&st, 0..=7);
    assert_eq!(cache.stats().invalidations, 1);
    assert!(cache.stats().lines_lexed > lexed, "re-lexed with the new rules");
    assert!(cache.entry_state(3, new_id).is_some());
    assert_eq!(fg_at(&h.highlight_document_line(&st, 3, DOC[3]), 0), Some(Color::Cyan));
}

#[test]
fn shared_registry_between_clones() -> Result<(), Box<dyn std::error::Error>> {
    let h = highlighter();
    let clone = h.clone();
    let before = h.rules_id().unwrap();
    clone.register_language("rust", RUST_OVERRIDE)?;
    assert_ne!(h.rules_id().unwrap(), before, "clones share the registry");

    Ok(())
}

#[test]
fn lazy_language_through_the_highlighter_loader() {
    let mut h = HighlightPlugin::new();
    h.global_theme.set_color("keyword", Color::Red);
    let h = h
        .with_language("markdown", MARKDOWN, None)
        .with_loader(|name| (name == "python").then(|| PYTHON.to_string()));

    let text = "```python\ndef f(): pass\n```";
    let st = state(text);
    h.prepare_viewport(&st, 0..=2);
    assert_eq!(fg_at(&h.highlight_document_line(&st, 1, "def f(): pass"), 0), Some(Color::Red));
    assert!(h.registry().lang_id("python").is_some());
}
