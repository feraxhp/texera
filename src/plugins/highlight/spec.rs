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

//! Language specifications and file loaders.

use std::path::PathBuf;
use std::fs;

use super::lang::CompiledLanguage;
use super::lang::SyntaxError;
use super::theme::ThemeMap;
use super::lang;

/// Source of a language (TOML) together with its theme.
///
/// # Examples
///
/// ```rust
/// use texera::LanguageToml;
/// use texera::ThemeMap;
///
/// let toml = "[meta]\nname = \"mini\"\n[states.root]\nrules = []";
/// let lang_def = LanguageToml::new("mini", toml, ThemeMap::default());
/// assert!(lang_def.compile().is_ok());
/// ```
#[derive(Clone, Debug)]
pub struct LanguageToml {
    /// Language identifier name.
    pub name: String,
    /// TOML definition source string, see [`lang`].
    pub source: String,
    /// Associated syntax theme for token coloring.
    pub theme: ThemeMap,
}

impl LanguageToml {
    /// Creates a new [`LanguageToml`] definition pairing a name, TOML source, and theme.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageToml;
    /// use texera::ThemeMap;
    ///
    /// let def = LanguageToml::new("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []", ThemeMap::default());
    /// assert_eq!(def.name, "test");
    /// ```
    pub fn new(name: impl Into<String>, source: impl Into<String>, theme: ThemeMap) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            theme,
        }
    }

    /// Compiles the definition without registering it, to validate it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageToml;
    /// use texera::ThemeMap;
    ///
    /// let def = LanguageToml::new("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []", ThemeMap::default());
    /// assert!(def.compile().is_ok());
    /// ```
    pub fn compile(&self) -> Result<CompiledLanguage, SyntaxError> {
        lang::compile(&self.source)
    }
}

/// Complete definition of a dynamically loaded language pairing grammar and optional theme.
///
/// # Examples
///
/// ```rust
/// use texera::LanguageSpec;
/// use texera::ThemeMap;
///
/// let spec = LanguageSpec::new("[meta]\nname = \"toy\"\n[states.root]\nrules = []", None);
/// assert!(spec.theme.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSpec {
    /// TOML syntax grammar definition.
    pub grammar: String,
    /// Optional syntax theme mapping token names to display colors.
    pub theme: Option<ThemeMap>,
}

impl LanguageSpec {
    /// Creates a new [`LanguageSpec`] pairing a grammar and optional theme.
    pub fn new(grammar: impl Into<String>, theme: Option<ThemeMap>) -> Self {
        Self {
            grammar: grammar.into(),
            theme,
        }
    }
}

/// Returns a [`Loader`] that reads `{dir}/{lang}.toml` or `{dir}/{lang}/{lang}.toml`.
///
/// Compose it with embedded definitions to get *"user files first, built-in as fallback"*:
///
/// ```ignore
/// let user = texera::dir_loader("~/.config/app/syntaxes");
/// highlight.with_loader(move |name| user(name).or_else(|| builtin(name)));
/// ```
pub fn dir_loader(dir: impl Into<PathBuf>) -> impl Fn(&str) -> Option<String> + Send + Sync + 'static {
    let dir: PathBuf = dir.into();
    move |name: &str| {
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return None;
        }

        fs::read_to_string(dir.join(format!("{name}.toml")))
            .or_else(|_| fs::read_to_string(dir.join(name).join(format!("{name}.toml"))))
            .ok()
    }
}

/// Returns a loader that loads `{dir}/{lang}.toml` and optionally `{dir}/{lang}.json` (or `{dir}/{lang}/theme.json`).
///
/// This loader can be passed to [`HighlightPlugin::with_spec_loader`] to dynamically provide both syntax
/// grammars and contextual themes on demand.
///
/// # Examples
///
/// ```rust,no_run
/// use texera::dir_spec_loader;
/// use texera::HighlightPlugin;
///
/// let plugin = HighlightPlugin::new().with_spec_loader(dir_spec_loader("~/.config/app/syntaxes"));
/// ```
pub fn dir_spec_loader(
    dir: impl Into<PathBuf>,
) -> impl Fn(&str) -> Option<LanguageSpec> + Send + Sync + 'static {
    let dir: PathBuf = dir.into();
    move |name: &str| {
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return None;
        }

        let direct_toml = dir.join(format!("{name}.toml"));
        if direct_toml.is_file() {
            let grammar = fs::read_to_string(&direct_toml).ok()?;
            let theme = ThemeMap::load_from_file(dir.join(format!("{name}.json"))).ok();
            return Some(LanguageSpec { grammar, theme, });
        }

        let sub = dir.join(name);
        if sub.is_dir() {
            let preferred_tomls = [format!("{name}.toml"), "syntax.toml".to_string()];
            let toml_path = preferred_tomls
                .iter()
                .map(|f| sub.join(f))
                .find(|p| p.is_file())
                .or_else(|| {
                    fs::read_dir(&sub)
                        .ok()?
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .find(|p| {
                            p.is_file()
                                && p.extension()
                                    .and_then(|e| e.to_str())
                                    .map(str::to_ascii_lowercase)
                                    .as_deref()
                                    == Some("toml")
                        })
                })?;

            let grammar = fs::read_to_string(toml_path).ok()?;
            let preferred_themes = ["theme.json".to_string(), format!("{name}.json")];
            let theme_path = preferred_themes
                .iter()
                .map(|f| sub.join(f))
                .find(|p| p.is_file())
                .or_else(|| {
                    fs::read_dir(&sub)
                        .ok()?
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .find(|p| {
                            p.is_file()
                                && p.extension()
                                    .and_then(|e| e.to_str())
                                    .map(str::to_ascii_lowercase)
                                    .as_deref()
                                    == Some("json")
                        })
                });
            let theme = theme_path.and_then(|p| ThemeMap::load_from_file(p).ok());
            return Some(LanguageSpec { grammar, theme, });
        }

        None
    }
}
