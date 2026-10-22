//! Procedural macros for the Texera editor ecosystem.

use proc_macro::TokenStream;
use std::fs;
use std::path::Path;

/// Scans `dir` (relative to `CARGO_MANIFEST_DIR`) at compile time and embeds every language found:
/// `dir/<lang>/<lang>.toml` (or `syntax.toml` / any `*.toml`) plus an optional `theme.json`.
/// Evaluates to a `HighlightPlugin`.
#[proc_macro]
pub fn include_languaje_dir(input: TokenStream) -> TokenStream {
    let raw_input = input.to_string();
    let trimmed = raw_input.trim();
    let dir_rel = trimmed.trim_matches('"').trim();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base_path = Path::new(&manifest_dir).join(dir_rel);

    let mut registrations = String::new();

    if base_path.is_dir() {
        if let Ok(entries) = fs::read_dir(&base_path) {
            let mut subdirs: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            subdirs.sort_by_key(|e| e.path());

            for entry in subdirs {
                let sub_path = entry.path();
                if sub_path.is_dir() {
                    let lang_name = sub_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    // Search for the TOML language definition: `<lang>.toml`, `syntax.toml`,
                    // or the first `*.toml` (sorted by name).
                    let mut scm_file = None;
                    let preferred = [
                        sub_path.join(format!("{lang_name}.toml")),
                        sub_path.join("syntax.toml"),
                    ];

                    if let Some(found) = preferred.iter().find(|p| p.is_file()) {
                        scm_file = Some(found.clone());
                    } else if let Ok(files) = fs::read_dir(&sub_path) {
                        let mut candidates: Vec<_> = files
                            .flatten()
                            .map(|f| f.path())
                            .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "toml"))
                            .collect();
                        candidates.sort();
                        scm_file = candidates.into_iter().next();
                    }

                    if let Some(scm_path) = scm_file {
                        let scm_str = scm_path.to_str().unwrap().replace('\\', "/");
                        let json_path = sub_path.join("theme.json");

                        if json_path.is_file() {
                            let json_str = json_path.to_str().unwrap().replace('\\', "/");
                            registrations.push_str(&format!(
                                r#"
                                {{
                                    let __src = include_str!("{scm_str}");
                                    let __theme = ::texera::ThemeMap::from_json_str(include_str!("{json_str}")).ok();
                                    __highlighter = __highlighter.with_language("{lang_name}", __src, __theme);
                                }}
                                "#
                            ));
                        } else {
                            registrations.push_str(&format!(
                                r#"
                                {{
                                    let __src = include_str!("{scm_str}");
                                    __highlighter = __highlighter.with_language("{lang_name}", __src, None);
                                }}
                                "#
                            ));
                        }
                    }
                }
            }
        }
    }

    let expanded = format!(
        r#"
        {{
            let mut __highlighter = ::texera::HighlightPlugin::new();
            {registrations}
            __highlighter
        }}
        "#
    );

    expanded.parse().unwrap()
}
