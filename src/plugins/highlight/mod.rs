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

//! Incremental syntax highlighting driven by declarative TOML state machines.
//!
//! * [`lang`] — the TOML format and its compiler.
//! * [`registry`] — [`LanguageRegistry`]: compiled languages, interned state stacks and the line
//!   lexer ([`LexState`] is a 4 byte handle to the stack of open constructs).
//! * [`syntax_cache`] — per-document incremental cache of the entry state of every line.
//!
//! See [`HighlightPlugin`] for the plugin plugged into the editor view.

use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use ratatui::style::{Color, Style};
use texera_core::{Extension, State};

use crate::{Highlight, ViewPlugin, ViewUpdate};

pub mod lang;
pub mod registry;
pub mod syntax_cache;
pub use lang::{
    CompiledLanguage, DOC_START_STATE, DOC_START_STATE_ID, ROOT_STATE, ROOT_STATE_ID, SyntaxError,
    TokenName,
};
pub use registry::{
    LangId, LanguageRegistry, LexState, Loader, MAX_DEPTH, MAX_POOL_STATES, PLAIN_LANG, Registered,
    ScopedToken, StateFrame,
};
pub use syntax_cache::{SyntaxCache, SyntaxField, SyntaxProgress, SyntaxStats};

/// Default number of lines past the last visible one whose entry state is kept ready.
pub const DEFAULT_LOOKAHEAD_LINES: usize = 2;
/// Default maximum number of lines lexed per [`Highlight::prepare_viewport`] call.
pub const DEFAULT_PARSE_BUDGET: usize = 5_000;

/// Syntax highlighting theme mapping token names to `ratatui::style::Color`.
///
/// Supports hierarchical token fallback (e.g. `keyword.control` falls back to `keyword`).
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use texera::ThemeMap;
///
/// let mut theme = ThemeMap::default();
/// theme.set_color("keyword", Color::Magenta);
/// assert_eq!(theme.color_for("keyword.control"), Some(Color::Magenta));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeMap {
    /// Mapping from token category name to terminal display color.
    pub token_colors: HashMap<String, Color>,
}

impl ThemeMap {
    /// Load theme mapping from a JSON file on disk.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use texera::ThemeMap;
    ///
    /// let theme = ThemeMap::load_from_file("themes/dark.json");
    /// ```
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Self::from_json_str(&content)
    }

    /// Load theme mapping from a JSON string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let json = r#"{"keyword": "Magenta", "string": "Green"}"#;
    /// let theme = ThemeMap::from_json_str(json).unwrap();
    /// assert_eq!(theme.color_for("keyword"), Some(Color::Magenta));
    /// ```
    pub fn from_json_str(json_content: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let raw_map: HashMap<String, String> = serde_json::from_str(json_content)?;
        let mut token_colors = HashMap::new();
        for (token, color_str) in raw_map {
            if let Ok(color) = Color::from_str(&color_str) {
                token_colors.insert(token, color);
            }
        }
        Ok(Self { token_colors })
    }

    /// Resolves the color of a token with a descending hierarchical fallback.
    ///
    /// `keyword.operator.arithmetic` is looked up as `keyword.operator.arithmetic`, then
    /// `keyword.operator`, then `keyword``.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let mut theme = ThemeMap::default();
    /// theme.set_color("keyword", Color::Magenta);
    /// assert_eq!(theme.color_for("keyword.control"), Some(Color::Magenta));
    /// assert_eq!(theme.color_for("unknown"), None);
    /// ```
    pub fn color_for(&self, token: &str) -> Option<Color> {
        let mut key = token;
        loop {
            if let Some(color) = self.token_colors.get(key) {
                return Some(*color);
            }
            key = key.rsplit_once('.')?.0;
        }
    }

    /// Assigns a display color to a specific token name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let mut theme = ThemeMap::default();
    /// theme.set_color("function.call", Color::Cyan);
    /// assert_eq!(theme.color_for("function.call"), Some(Color::Cyan));
    /// ```
    pub fn set_color(&mut self, token: impl Into<String>, color: Color) {
        self.token_colors.insert(token.into(), color);
    }
}

/// Source of a language (TOML) together with its theme.
///
/// # Examples
///
/// ```rust
/// use texera::{LanguageToml, ThemeMap};
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
    /// use texera::{LanguageToml, ThemeMap};
    ///
    /// let def = LanguageToml::new("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []", ThemeMap::default());
    /// assert_eq!(def.name, "test");
    /// ```
    pub fn new(name: impl Into<String>, source: impl Into<String>, theme: ThemeMap) -> Self {
        Self { name: name.into(), source: source.into(), theme }
    }

    /// Compiles the definition without registering it, to validate it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageToml, ThemeMap};
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
/// use texera::{LanguageSpec, ThemeMap};
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
        if 
            name.is_empty() 
            || name.contains(['/', '\\']) 
            || name.contains("..") 
        { return None; }
        
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
/// use texera::{HighlightPlugin, dir_spec_loader};
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
            return Some(LanguageSpec { grammar, theme });
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
            return Some(LanguageSpec { grammar, theme });
        }

        None
    }
}

/// Syntax highlight plugin powered by TOML state machines and JSON themes.
///
/// Cloning is cheap and clones **share the language registry**: a language registered through one
/// clone is visible to all of them.
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use texera::{HighlightPlugin, ThemeMap};
///
/// let toml = r#"
/// [meta]
/// name = "toy"
/// extensions = ["toy"]
/// [states.root]
/// rules = [{ match = '\b\d+\b', token = "number" }]
/// "#;
///
/// let mut theme = ThemeMap::default();
/// theme.set_color("number", Color::Yellow);
///
/// let highlight = HighlightPlugin::new().with_language("toy", toml, Some(theme));
/// ```
#[derive(Clone, Debug)]
pub struct HighlightPlugin {
    /// Default fallback theme applied when a language has no custom override for a token.
    pub global_theme: ThemeMap,
    /// Languages registered through this highlighter (sources and themes).
    pub languages: HashMap<String, LanguageToml>,
    /// Currently active language name used to tokenize documents.
    pub active_language: Option<String>,
    /// Lines past the last visible one whose lexical state is kept ready (default `2`).
    pub lookahead_lines: usize,
    /// Maximum number of lines lexed per frame while catching up with the viewport.
    pub parse_budget: usize,
    /// Languages that failed to compile (`"name: reason"`), kept for diagnostics.
    pub errors: Vec<String>,
    dynamic_themes: Arc<RwLock<HashMap<String, Arc<ThemeMap>>>>,
    registry: Arc<LanguageRegistry>,
}

impl Default for HighlightPlugin {
    fn default() -> Self {
        Self {
            global_theme: ThemeMap::default(),
            languages: HashMap::new(),
            active_language: None,
            lookahead_lines: DEFAULT_LOOKAHEAD_LINES,
            parse_budget: DEFAULT_PARSE_BUDGET,
            errors: Vec::new(),
            dynamic_themes: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(LanguageRegistry::new()),
        }
    }
}

impl HighlightPlugin {
    /// Creates a new default [`HighlightPlugin`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with an initial TOML language (registered as `"default"`) and optional theme.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let toml = "[meta]\nname = \"default\"\n[states.root]\nrules = []";
    /// let plugin = HighlightPlugin::with_specs((toml, None));
    /// ```
    pub fn with_specs(pair: (impl Into<String>, Option<ThemeMap>)) -> Self {
        let (source, theme) = pair;
        Self::default().with_language("default", source, theme)
    }

    /// The registry shared by every clone of this highlighter.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new();
    /// let registry = plugin.registry();
    /// assert_eq!(registry.revision(), 1);
    /// ```
    pub fn registry(&self) -> &Arc<LanguageRegistry> {
        &self.registry
    }

    fn add_language(
        &mut self,
        name: String,
        source: String,
        theme: Option<ThemeMap>,
    ) -> Result<(), SyntaxError> {
        self.registry.register(&name, &source)?;
        let theme = theme.unwrap_or_default();
        if self.active_language.is_none() {
            self.active_language = Some(name.clone());
        }
        self.languages.insert(name.clone(), LanguageToml { name, source, theme });
        Ok(())
    }

    /// Registers a TOML language, returning an error if it does not compile.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let toml = "[meta]\nname = \"toy\"\n[states.root]\nrules = []";
    /// let plugin = HighlightPlugin::new().try_with_language("toy", toml, None).unwrap();
    /// ```
    pub fn try_with_language(
        mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        theme: Option<ThemeMap>,
    ) -> Result<Self, SyntaxError> {
        self.add_language(name.into(), source.into(), theme)?;
        Ok(self)
    }

    /// Registers a TOML language (replacing a previous one with the same name). A definition that
    /// does not compile is skipped and reported in [`HighlightPlugin::errors`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let toml = "[meta]\nname = \"toy\"\n[states.root]\nrules = []";
    /// let plugin = HighlightPlugin::new().with_language("toy", toml, None);
    /// assert!(plugin.errors.is_empty());
    /// ```
    pub fn with_language(
        mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        theme: Option<ThemeMap>,
    ) -> Self {
        let name = name.into();
        if let Err(e) = self.add_language(name.clone(), source.into(), theme) {
            self.errors.push(format!("{name}: {e}"));
        }
        self
    }

    /// Registers a [`LanguageToml`] definition.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{HighlightPlugin, LanguageToml, ThemeMap};
    ///
    /// let def = LanguageToml::new("toy", "[meta]\nname = \"toy\"\n[states.root]\nrules = []", ThemeMap::default());
    /// let plugin = HighlightPlugin::new().with_language_def(def);
    /// ```
    pub fn with_language_def(self, def: LanguageToml) -> Self {
        let theme = Some(def.theme);
        self.with_language(def.name, def.source, theme)
    }

    /// Installs the on-demand loader used for `embed`ded languages that are not registered, see
    /// [`LanguageRegistry::set_loader`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new().with_loader(|_name| None);
    /// ```
    pub fn with_loader(self, loader: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Self {
        self.registry.set_loader(loader);
        self
    }

    /// Allows the application to dynamically provide both grammar syntax and an optional theme
    /// on demand for embedded languages.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{HighlightPlugin, LanguageSpec};
    ///
    /// let plugin = HighlightPlugin::new().with_spec_loader(|name| {
    ///     if name == "dummy" {
    ///         Some(LanguageSpec::new("[meta]\nname = \"dummy\"\n[states.root]\nrules = []", None))
    ///     } else {
    ///         None
    ///     }
    /// });
    /// ```
    pub fn with_spec_loader(
        self,
        loader: impl Fn(&str) -> Option<LanguageSpec> + Send + Sync + 'static,
    ) -> Self {
        let loader = Arc::new(loader);
        let dt = self.dynamic_themes.clone();
        self.registry.set_loader(move |name| {
            let spec = loader(name)?;
            if let Some(theme) = spec.theme {
                dt.write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(name.trim().to_ascii_lowercase(), Arc::new(theme));
            }
            Some(spec.grammar)
        });
        self
    }

    /// Registers or overrides a language **at runtime** through the shared registry (hot reload,
    /// asynchronous loading, ...). Returns the new registry revision: every cached parse made with
    /// an older one is discarded on the next [`Highlight::prepare_viewport`], so the caller only
    /// has to request a repaint.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new();
    /// let toml = "[meta]\nname = \"toy\"\n[states.root]\nrules = []";
    /// let rev = plugin.register_language("toy", toml).unwrap();
    /// assert!(rev > 0);
    /// ```
    pub fn register_language(&self, name: &str, source: &str) -> Result<u64, SyntaxError> {
        Ok(self.registry.register(name, source)?.revision)
    }

    /// Set active language for highlighting (a registered name or file extension).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let mut plugin = HighlightPlugin::new().with_language("toy", "[meta]\nname = \"toy\"\n[states.root]\nrules = []", None);
    /// plugin.set_active_language("toy");
    /// assert_eq!(plugin.active().unwrap().1, "toy");
    /// ```
    pub fn set_active_language(&mut self, name: &str) {
        if self.registry.lang_id(name).is_some() {
            self.active_language = Some(name.to_string());
        }
    }

    /// Sets how many lines past the last visible one keep their lexical state ready.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new().with_lookahead(5);
    /// assert_eq!(plugin.lookahead_lines, 5);
    /// ```
    pub fn with_lookahead(mut self, lines: usize) -> Self {
        self.lookahead_lines = lines;
        self
    }

    /// Sets the maximum number of lines lexed per frame while catching up with the viewport.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new().with_parse_budget(2_000);
    /// assert_eq!(plugin.parse_budget, 2_000);
    /// ```
    pub fn with_parse_budget(mut self, lines: usize) -> Self {
        self.parse_budget = lines;
        self
    }

    /// Extension to register in the `State` so the lexical parse is cached **per document
    /// version** (and follows typing, undo and redo). Without it the highlighter still works
    /// but every line is lexed as if no multi-line construct was open.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    /// use texera_core::State;
    ///
    /// let state = State::from("fn foo() {}").with_extensions([HighlightPlugin::syntax_extension()]);
    /// ```
    pub fn syntax_extension() -> Extension {
        SyntaxField::extension()
    }

    /// How far the lexical parse of `state` has advanced (`None` if
    /// [`HighlightPlugin::syntax_extension`] is not registered). Useful to know where the parse is
    /// after jumping far away in a big file.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    /// use texera_core::State;
    ///
    /// let state = State::from("code").with_extensions([HighlightPlugin::syntax_extension()]);
    /// let progress = HighlightPlugin::progress(&state).unwrap();
    /// assert_eq!(progress.total_lines, 1);
    /// ```
    pub fn progress(state: &State) -> Option<SyntaxProgress> {
        state.field::<SyntaxField>().map(|cache| cache.progress())
    }

    /// Instance version of [`HighlightPlugin::progress`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    /// use texera_core::State;
    ///
    /// let plugin = HighlightPlugin::new();
    /// let state = State::from("code").with_extensions([HighlightPlugin::syntax_extension()]);
    /// assert!(plugin.parse_progress(&state).is_some());
    /// ```
    pub fn parse_progress(&self, state: &State) -> Option<SyntaxProgress> {
        Self::progress(state)
    }

    /// Id (inside [`HighlightPlugin::registry`]) and name of the language used for highlighting.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let toml = "[meta]\nname = \"demo\"\n[states.root]\nrules = []";
    /// let plugin = HighlightPlugin::new().with_language("demo", toml, None);
    /// assert_eq!(plugin.active().unwrap().1, "demo");
    /// ```
    pub fn active(&self) -> Option<(LangId, &str)> {
        if let Some(name) = self.active_language.as_deref() {
            if let Some(id) = self.registry.lang_id(name) {
                return Some((id, name));
            }
        }
        self.languages
            .keys()
            .filter_map(|name| self.registry.lang_id(name).map(|id| (id, name.as_str())))
            .min_by_key(|(_, name)| *name)
    }

    /// Identity of the rules the cache of the active language depends on (see
    /// [`SyntaxCache::entry_state`]).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::HighlightPlugin;
    ///
    /// let toml = "[meta]\nname = \"demo\"\n[states.root]\nrules = []";
    /// let plugin = HighlightPlugin::new().with_language("demo", toml, None);
    /// assert!(plugin.rules_id().is_some());
    /// ```
    pub fn rules_id(&self) -> Option<u64> {
        self.active().map(|(id, _)| self.registry.rules_id(id))
    }

    /// Returns the theme associated with the given language identifier, if any.
    pub fn theme_of_lang_id(&self, lang_id: LangId) -> Option<Arc<ThemeMap>> {
        let name = self.registry.lang_name(lang_id)?;
        self.theme_of(&name)
    }

    /// Returns the theme associated with the given language name, if any.
    pub fn theme_of(&self, name: &str) -> Option<Arc<ThemeMap>> {
        let key = name.trim().to_ascii_lowercase();
        if let Some(l) = self.languages.get(name).or_else(|| self.languages.get(&key)) {
            if !l.theme.token_colors.is_empty() {
                return Some(Arc::new(l.theme.clone()));
            }
        }
        if let Some(dt) = self
            .dynamic_themes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            if !dt.token_colors.is_empty() {
                return Some(dt.clone());
            }
        }
        None
    }

    /// Resolves the style of every token of `line`, returning **char** ranges.
    fn styled_tokens(
        &self,
        host_lang: LangId,
        host_theme: Option<&ThemeMap>,
        line: &str,
        entry: LexState,
    ) -> Vec<(Range<usize>, Style)> {
        let mut state = entry;
        let token_ranges = self.registry.tokenize_line_stateful(host_lang, line, &mut state);
        let ascii = line.is_ascii();
        let mut styled_ranges = Vec::with_capacity(token_ranges.len());
        // Tokens are ordered, so byte -> char conversion only ever moves forward.
        let (mut last_byte, mut last_char) = (0usize, 0usize);
        let mut to_char = |byte: usize| -> usize {
            if ascii {
                return byte;
            }
            last_char += line[last_byte..byte].chars().count();
            last_byte = byte;
            last_char
        };

        let mut last_lang: LangId = u16::MAX;
        let mut last_theme: Option<Arc<ThemeMap>> = None;

        for (range, token_lang_id, token) in token_ranges {
            let color = if token_lang_id == host_lang {
                host_theme
                    .and_then(|t| t.color_for(&token))
                    .or_else(|| self.global_theme.color_for(&token))
            } else {
                if last_lang != token_lang_id {
                    last_lang = token_lang_id;
                    last_theme = self.theme_of_lang_id(token_lang_id);
                }
                if let Some(embed_theme) = &last_theme {
                    // Embedded context has its own theme: ONLY its theme is used, no fallback to host or global
                    embed_theme.color_for(&token)
                } else {
                    // Embedded context has no theme: fallback to host theme, then global theme
                    host_theme
                        .and_then(|t| t.color_for(&token))
                        .or_else(|| self.global_theme.color_for(&token))
                }
            };

            if let Some(c) = color {
                let start = to_char(range.start);
                let end = to_char(range.end);
                styled_ranges.push((start..end, Style::default().fg(c)));
            }
        }

        styled_ranges
    }

    /// Extends the highlighter at runtime by scanning a directory.
    ///
    /// Two layouts are understood (and may be mixed):
    ///
    /// * `root/<lang>.toml` with an optional `root/<lang>.json` theme;
    /// * `root/<lang>/` containing a `*.toml` (preferably `<lang>.toml` / `syntax.toml`) and an
    ///   optional `theme.json` (or any `*.json`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new().extend_from("syntaxes").unwrap();
    /// ```
    pub fn extend_from(mut self, root_dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let root = root_dir.as_ref();
        if !root.exists() || !root.is_dir() {
            return Ok(self);
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(root)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();

        let ext_of = |p: &Path| p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let stem_of = |p: &Path| p.file_stem().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();

        for path in entries {
            if path.is_file() && ext_of(&path).as_deref() == Some("toml") {
                let name = stem_of(&path);
                let theme = ThemeMap::load_from_file(path.with_extension("json")).ok();
                self = self.with_language(name, fs::read_to_string(&path)?, theme);
            } else if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                let mut files: Vec<PathBuf> =
                    fs::read_dir(&path)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
                files.sort();

                let preferred = [format!("{name}.toml"), "syntax.toml".to_string()];
                let toml = preferred
                    .iter()
                    .map(|f| path.join(f))
                    .find(|p| p.is_file())
                    .or_else(|| files.iter().find(|p| ext_of(p).as_deref() == Some("toml")).cloned());
                let theme_path = Some(path.join("theme.json"))
                    .filter(|p| p.is_file())
                    .or_else(|| files.iter().find(|p| ext_of(p).as_deref() == Some("json")).cloned());

                if let Some(toml) = toml {
                    let theme = theme_path.and_then(|p| ThemeMap::load_from_file(p).ok());
                    self = self.with_language(name, fs::read_to_string(&toml)?, theme);
                }
            }
        }

        Ok(self)
    }
}

impl ViewPlugin for HighlightPlugin {
    fn update(&mut self, _update: &ViewUpdate) {}
}

impl Highlight for HighlightPlugin {
    /// Stateless highlighting: the line is lexed as if no multi-line construct was open.
    /// Returned ranges are char offsets inside `line`.
    fn highlight_line<'a>(
        &self,
        line: &'a str,
        line_index: usize,
    ) -> Vec<(Range<usize>, Style)> {
        let Some((lang, name)) = self.active() else {
            return Vec::new();
        };
        let entry = if line_index == 0 { LexState::DOC_START } else { LexState::BASE };
        let host_theme = self.theme_of(name);
        self.styled_tokens(lang, host_theme.as_deref(), line, entry)
    }

    /// Advances the cached parse just enough to cover the visible lines plus the lookahead.
    fn prepare_viewport(&self, state: &State, visible_lines: std::ops::RangeInclusive<usize>) {
        let (Some((lang, _)), Some(cache)) = (self.active(), state.field::<SyntaxField>()) else {
            return;
        };
        let through = visible_lines.end().saturating_add(self.lookahead_lines);
        cache.advance(state.doc(), &self.registry, lang, through, self.parse_budget);
    }

    /// Cache-aware highlighting: continues from the cached entry state of `line_index`.
    /// If the parse has not reached that line yet it falls back to a stateless lex.
    fn highlight_document_line(
        &self,
        state: &State,
        line_index: usize,
        line: &str,
    ) -> Vec<(Range<usize>, Style)> {
        let Some((lang, name)) = self.active() else {
            return Vec::new();
        };
        let fallback = if line_index == 0 { LexState::DOC_START } else { LexState::BASE };
        let entry = state
            .field::<SyntaxField>()
            .and_then(|cache| cache.entry_state(line_index, self.registry.rules_id(lang)))
            .unwrap_or(fallback);
        let host_theme = self.theme_of(name);
        self.styled_tokens(lang, host_theme.as_deref(), line, entry)
    }
}

/// Convenience macro to embed a single language's TOML and optional theme.json at compile-time.
///
/// # Examples
///
/// ```rust,ignore
/// use texera::include_toml_language;
///
/// let lang = include_toml_language!("rust", "../syntaxes/rust.toml");
/// ```
#[macro_export]
macro_rules! include_toml_language {
    ($lang:expr, $toml_file:expr) => {
        $crate::LanguageToml::new(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::default(),
        )
    };
    ($lang:expr, $toml_file:expr, $json_file:expr) => {
        $crate::LanguageToml::new(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::from_json_str(include_str!($json_file)).unwrap_or_default(),
        )
    };
    ($highlighter:expr, $lang:expr, $toml_file:expr) => {
        $highlighter.with_language($lang, include_str!($toml_file), None)
    };
    ($highlighter:expr, $lang:expr, $toml_file:expr, $json_file:expr) => {
        $highlighter.with_language(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::from_json_str(include_str!($json_file)).ok(),
        )
    };
}

/// Backwards compatibility alias for [`include_toml_language!`].
#[macro_export]
macro_rules! include_scm_language {
    ($($arg:tt)*) => {
        $crate::include_toml_language!($($arg)*)
    };
}
