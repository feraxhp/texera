//! Category 6: hierarchical color lookup in `Theme::color_for`.

mod common;

use common::*;
use ratatui::style::Color;
use texera::{
    dir_spec_loader, Highlight, HighlightPlugin, LanguageRegistry, LanguageSpec, ThemeMap,
};
use texera_core::State;

#[test]
fn test_multilevel_descending_fallback_with_rsplit_once() {
    let mut theme = ThemeMap::default();
    theme.set_color("keyword", Color::Red);
    theme.set_color("keyword.operator", Color::Blue);

    // 1. Exact match.
    assert_eq!(theme.color_for("keyword"), Some(Color::Red));
    assert_eq!(theme.color_for("keyword.operator"), Some(Color::Blue));

    // 2. Three segments falling back to two: "keyword.operator.arithmetic" -> "keyword.operator" -> Blue.
    assert_eq!(
        theme.color_for("keyword.operator.arithmetic"),
        Some(Color::Blue),
        "`keyword.operator.arithmetic` must fall back to `keyword.operator`"
    );

    // 3. Three segments falling back across missing middle segment:
    //    "keyword.control.conditional" -> "keyword.control" (not found) -> "keyword" -> Red.
    assert_eq!(
        theme.color_for("keyword.control.conditional"),
        Some(Color::Red),
        "`keyword.control.conditional` must fall back to `keyword`"
    );

    // 4. Four segments: "a.b.c.d" -> falls all the way down to "a".
    theme.set_color("a", Color::Green);
    assert_eq!(theme.color_for("a.b.c.d"), Some(Color::Green));

    // 5. Unknown token type: returns None.
    assert_eq!(theme.color_for("unknown.token.type"), None);
    assert_eq!(theme.color_for("variable"), None);
}

#[test]
fn test_theme_json_parsing_and_fallback() {
    let json = r#"{
        "keyword": "yellow",
        "keyword.function": "magenta",
        "function": "blue",
        "function.call": "light_blue"
    }"#;
    let theme = ThemeMap::from_json_str(json).expect("Theme JSON should parse");

    assert_eq!(theme.color_for("keyword.function"), Some(Color::Magenta));
    assert_eq!(theme.color_for("keyword.conditional"), Some(Color::Yellow));
    assert_eq!(theme.color_for("function.call.method"), Some(Color::LightBlue));
    assert_eq!(theme.color_for("function.builtin"), Some(Color::Blue));
    assert_eq!(theme.color_for("number"), None);
}

#[test]
fn test_context_specific_theme_and_no_fallback_when_theme_present() {
    let mut md_theme = ThemeMap::default();
    md_theme.set_color("markup.heading", Color::Red);
    md_theme.set_color("punctuation.delimiter", Color::Yellow);
    // Even if host theme defines keyword, the embedded language should use its own!
    md_theme.set_color("keyword", Color::DarkGray);

    let mut rust_theme = ThemeMap::default();
    rust_theme.set_color("keyword", Color::Cyan);

    let mut global_theme = ThemeMap::default();
    global_theme.set_color("function", Color::Green);

    let mut h = HighlightPlugin::new();
    h.global_theme = global_theme;
    h = h
        .with_language("markdown", MARKDOWN, Some(md_theme))
        .with_language("rust", RUST, Some(rust_theme));
    h.set_active_language("markdown");

    let state = State::from("```rust\nfn main() {}\n```").with_extensions([HighlightPlugin::syntax_extension()]);
    h.prepare_viewport(&state, 0..=2);

    // Line 0: ```rust
    let styled0 = h.highlight_document_line(&state, 0, "```rust");
    assert_eq!(styled0[0].1.fg, Some(Color::Yellow)); // punctuation.delimiter from markdown

    // Line 1: fn main() {}
    let styled1 = h.highlight_document_line(&state, 1, "fn main() {}");
    // "fn" matches keyword -> should be Cyan from rust_theme, NOT DarkGray from md_theme
    let fn_styled = styled1.iter().find(|(r, _)| r == &(0..2)).unwrap();
    assert_eq!(fn_styled.1.fg, Some(Color::Cyan));

    // "main" is function.definition (fallback: function).
    // In rust_theme, "function" is NOT defined.
    // Since rust HAS a theme, it must NOT fallback to global_theme (Green) or md_theme!
    // So "main" must NOT have fg = Some(Green).
    let main_styled = styled1.iter().find(|(r, _)| r == &(3..7));
    assert!(main_styled.is_none() || main_styled.unwrap().1.fg.is_none());

    // Line 2: ``` (closing delimiter)
    // The closing delimiter is emitted with parent LangId (markdown), so it gets Yellow from md_theme!
    let styled2 = h.highlight_document_line(&state, 2, "```");
    assert_eq!(styled2[0].1.fg, Some(Color::Yellow));
}

#[test]
fn test_fallback_to_host_and_global_when_embedded_has_no_theme() {
    let mut md_theme = ThemeMap::default();
    md_theme.set_color("punctuation.delimiter", Color::Yellow);
    md_theme.set_color("keyword", Color::Magenta);

    let mut global_theme = ThemeMap::default();
    global_theme.set_color("function", Color::Green);

    // Register rust without theme (None)
    let mut h = HighlightPlugin::new();
    h.global_theme = global_theme;
    h = h
        .with_language("markdown", MARKDOWN, Some(md_theme))
        .with_language("rust", RUST, None);
    h.set_active_language("markdown");

    let state = State::from("```rust\nfn main() {}\n```").with_extensions([HighlightPlugin::syntax_extension()]);
    h.prepare_viewport(&state, 0..=2);

    let styled1 = h.highlight_document_line(&state, 1, "fn main() {}");
    // "fn" is keyword -> rust has no theme, so falls back to host theme (Magenta)!
    let fn_styled = styled1.iter().find(|(r, _)| r == &(0..2)).unwrap();
    assert_eq!(fn_styled.1.fg, Some(Color::Magenta));

    // "main" is function.definition -> not in md_theme, so falls back to global_theme (Green)!
    let main_styled = styled1.iter().find(|(r, _)| r == &(3..7)).unwrap();
    assert_eq!(main_styled.1.fg, Some(Color::Green));
}

#[test]
fn test_with_spec_loader_dynamic_theming() {
    let mut md_theme = ThemeMap::default();
    md_theme.set_color("punctuation.delimiter", Color::Yellow);

    let mut rust_theme = ThemeMap::default();
    rust_theme.set_color("keyword", Color::Blue);

    let rust_th = rust_theme.clone();
    let h = HighlightPlugin::new()
        .with_language("markdown", MARKDOWN, Some(md_theme))
        .with_spec_loader(move |name| {
            if name == "rust" {
                Some(LanguageSpec::new(RUST, Some(rust_th.clone())))
            } else {
                None
            }
        });

    let state = State::from("```rust\nfn main() {}\n```").with_extensions([HighlightPlugin::syntax_extension()]);
    h.prepare_viewport(&state, 0..=2);

    let styled1 = h.highlight_document_line(&state, 1, "fn main() {}");
    let fn_styled = styled1.iter().find(|(r, _)| r == &(0..2)).unwrap();
    assert_eq!(fn_styled.1.fg, Some(Color::Blue));
}

#[test]
fn test_dir_spec_loader_loads_grammar_and_theme() {
    let tmp = std::env::temp_dir().join("texera_spec_loader_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let toml_path = tmp.join("toy.toml");
    let json_path = tmp.join("toy.json");
    std::fs::write(&toml_path, "[meta]\nname = \"toy\"\n[states.root]\nrules = [{ match = 'hello', token = \"keyword\" }]").unwrap();
    std::fs::write(&json_path, r#"{"keyword": "Cyan"}"#).unwrap();

    let loader = dir_spec_loader(&tmp);
    let spec = loader("toy").expect("toy spec should load");
    assert!(spec.grammar.contains("toy"));
    assert_eq!(spec.theme.as_ref().and_then(|t| t.color_for("keyword")), Some(Color::Cyan));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_language_registry_lang_name() {
    let reg = LanguageRegistry::new();
    let reg_rust = reg.register("rust", "[meta]\nname = \"rust\"\n[states.root]\nrules = []").unwrap();
    assert_eq!(reg.lang_name(reg_rust.lang_id), Some("rust".to_string()));
    assert_eq!(reg.lang_name(999), None);
}
