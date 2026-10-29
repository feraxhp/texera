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

//! HighlightPlugin implementation and configuration.

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::Arc;

use texera_core::Extension;
use texera_core::State;

use super::syntax_cache::SyntaxProgress;
use super::registry::LanguageRegistry;
use super::syntax_cache::SyntaxField;
use super::spec::LanguageSpec;
use super::spec::LanguageToml;
use super::lang::SyntaxError;
use super::registry::LangId;
use super::theme::ThemeMap;

/// Default number of lines past the last visible one whose entry state is kept ready.
pub const DEFAULT_LOOKAHEAD_LINES: usize = 2;
/// Default maximum number of lines lexed per frame.
pub const DEFAULT_PARSE_BUDGET: usize = 5_000;

/// Syntax highlight plugin powered by TOML state machines and JSON themes.
///
/// Cloning is cheap and clones **share the language registry**: a language registered through one
/// clone is visible to all of them.
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use texera::HighlightPlugin;
/// use texera::ThemeMap;
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
    pub(crate) dynamic_themes: Arc<RwLock<HashMap<String, Arc<ThemeMap>>>>,
    pub(crate) registry: Arc<LanguageRegistry>,
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
        self.languages.insert(name.clone(), LanguageToml { name, source, theme, });
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
    /// use texera::HighlightPlugin;
    /// use texera::LanguageToml;
    /// use texera::ThemeMap;
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
    /// use texera::HighlightPlugin;
    /// use texera::LanguageSpec;
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
}
