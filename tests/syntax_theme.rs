//! Category 6: hierarchical color lookup in `Theme::color_for`.

use ratatui::style::Color;
use texera::ThemeMap;

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
