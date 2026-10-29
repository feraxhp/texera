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

//! Language registry and incremental, stack based line lexer.
//!
//! # Model
//!
//! Every language is a state machine (see [`super::lang`]). While lexing, the machine keeps a
//! **stack of [`StateFrame`]s**. The state carried from the end of a line to the start of the next
//! one is that whole stack, interned in an internal state pool so it travels as a 4 byte [`LexState`]:
//! comparing two stacks of any depth (which is what [`super::SyntaxCache`] does to resynchronise
//! after an edit) is a single `u32 == u32`.
//!
//! `LexState(0)` is always the base stack `[(active language, root)]`.
//! `LexState(1)` is always the document start stack `[(active language, doc_start)]`.

use std::sync::atomic::AtomicU64;
use std::sync::RwLockWriteGuard;
use std::sync::atomic::Ordering;
use std::sync::RwLockReadGuard;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::MutexGuard;
use std::sync::RwLock;
use std::sync::Mutex;
use std::ops::Range;
use std::sync::Arc;
use std::fmt;

pub use super::pool::MAX_POOL_STATES;
use super::lang::CompiledLanguage;
pub use super::pool::StateFrame;
pub use super::pool::MAX_DEPTH;
pub use super::pool::LexState;
use super::lang::SyntaxError;
use super::lang::TokenName;
use super::pool::StatePool;
use super::lang::EmbedEnd;
use super::lang;

/// Identifier of a language inside a [`LanguageRegistry`].
pub type LangId = u16;

/// A token with UTF-8 byte range, generating language identifier, and token name.
pub type ScopedToken = (Range<usize>, LangId, TokenName);

/// Pseudo language of an `embed` block whose language is not available: nothing is highlighted
/// until the block's delimiter is found.
pub const PLAIN_LANG: LangId = u16::MAX;

/// Loads the TOML source of a language by name (`"rust"`, `"python"`, ...), or `None`.
pub type Loader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Result of registering a language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Registered {
    /// Identifier allocated for the registered language.
    pub lang_id: LangId,
    /// Registry revision after the registration. Caches computed with an older one are discarded.
    pub revision: u64,
}

#[derive(Default)]
pub(crate) struct Langs {
    pub(crate) by_id: Vec<Arc<CompiledLanguage>>,
    pub(crate) names: Vec<String>,
    pub(crate) by_name: HashMap<String, LangId>,
    pub(crate) by_ext: HashMap<String, LangId>,
    /// Names the loader could not provide (so it is not asked again for every line).
    pub(crate) unavailable: HashSet<String>,
    pub(crate) embed_ends: Vec<Arc<EmbedEnd>>,
    pub(crate) embed_index: HashMap<(String, Option<String>), u16>,
}

impl Langs {
    fn intern_embed_end(&mut self, end: &Arc<EmbedEnd>) -> u16 {
        let key = (end.pattern.clone(), end.token.as_deref().map(str::to_string));
        if let Some(&id) = self.embed_index.get(&key) {
            return id;
        }
        self.embed_ends.push(end.clone());
        let id = u16::try_from(self.embed_ends.len()).unwrap_or(u16::MAX);
        self.embed_index.insert(key, id);
        id
    }
}

/// Shared set of compiled languages plus the state pool their stacks are interned in.
///
/// The registry is internally synchronised: wrap it in an [`Arc`] and share it freely.
pub struct LanguageRegistry {
    langs: RwLock<Langs>,
    pool: Mutex<StatePool>,
    revision: AtomicU64,
    loader: RwLock<Option<Loader>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for LanguageRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let langs = self.read_langs();
        f.debug_struct("LanguageRegistry")
            .field("languages", &langs.names)
            .field("revision", &self.revision())
            .field("pool_len", &self.pool_len())
            .finish()
    }
}

impl LanguageRegistry {
    /// Creates an empty registry.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// assert_eq!(registry.revision(), 1);
    /// ```
    pub fn new() -> Self {
        Self {
            langs: RwLock::new(Langs::default()),
            pool: Mutex::new(StatePool::new()),
            revision: AtomicU64::new(1),
            loader: RwLock::new(None),
        }
    }

    pub(crate) fn read_langs(&self) -> RwLockReadGuard<'_, Langs> {
        self.langs.read().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn write_langs(&self) -> RwLockWriteGuard<'_, Langs> {
        self.langs.write().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn pool(&self) -> MutexGuard<'_, StatePool> {
        self.pool.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Current revision. It changes whenever the meaning of a cached [`LexState`] may have
    /// changed: a language was registered / overridden, the loader changed, or the state pool was
    /// flushed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let rev = registry.revision();
    /// assert!(rev >= 1);
    /// ```
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub(crate) fn bump_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Identity of the rules a cache computed for `lang` depends on.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let id = registry.rules_id(0);
    /// assert!(id > 0);
    /// ```
    pub fn rules_id(&self, lang: LangId) -> u64 {
        (self.revision() << 16) | lang as u64
    }

    /// Number of distinct stacks currently interned (including the reserved base entries).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// assert!(registry.pool_len() >= 2);
    /// ```
    pub fn pool_len(&self) -> usize {
        self.pool().stacks.len()
    }

    // ---------------------------------------------------------------------------------------
    // Registration and lookup
    // ---------------------------------------------------------------------------------------

    /// Compiles `source` (TOML) and registers it under `name`, replacing any previous language
    /// with that name (it keeps its [`LangId`]). Bumps the revision: every cache derived from the
    /// registry is rebuilt on its next use.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let toml = "[meta]\nname = \"toy\"\n[states.root]\nrules = []";
    /// let reg = registry.register("toy", toml).unwrap();
    /// assert_eq!(reg.lang_id, 0);
    /// ```
    pub fn register(&self, name: &str, source: &str) -> Result<Registered, SyntaxError> {
        let lang = lang::compile(source)?;
        Ok(self.insert(name, lang, true))
    }

    /// Like [`LanguageRegistry::register`], naming the language after its `[meta] name`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let toml = "[meta]\nname = \"min\"\n[states.root]\nrules = []";
    /// let reg = registry.register_source(toml).unwrap();
    /// assert_eq!(registry.lang_id("min"), Some(reg.lang_id));
    /// ```
    pub fn register_source(&self, source: &str) -> Result<Registered, SyntaxError> {
        let lang = lang::compile(source)?;
        if lang.name().is_empty() {
            return Err(SyntaxError::new("the language has no `meta.name`"));
        }
        let name = lang.name().to_string();
        Ok(self.insert(&name, lang, true))
    }

    pub(crate) fn insert(&self, name: &str, mut lang: CompiledLanguage, replace: bool) -> Registered {
        let key = name.trim().to_ascii_lowercase();
        let mut langs = self.write_langs();
        if !replace {
            if let Some(&id) = langs.by_name.get(&key) {
                return Registered { lang_id: id, revision: self.revision(), };
            }
        }
        for spec in lang.embed_specs_mut() {
            spec.end_id = langs.intern_embed_end(&spec.end);
        }
        let lang = Arc::new(lang);
        let id = match langs.by_name.get(&key).copied() {
            Some(id) => {
                langs.by_id[id as usize] = lang.clone();
                id
            }
            None => {
                let id = LangId::try_from(langs.by_id.len())
                    .ok()
                    .filter(|id| *id < PLAIN_LANG)
                    .expect("too many languages");
                langs.by_id.push(lang.clone());
                langs.names.push(name.trim().to_string());
                langs.by_name.insert(key.clone(), id);
                id
            }
        };
        for ext in lang.extensions() {
            langs.by_ext.insert(ext.clone(), id);
        }
        langs.unavailable.remove(&key);
        drop(langs);

        if replace {
            // Stacks interned so far may mention states that no longer exist.
            self.pool().clear();
            let revision = self.bump_revision();
            Registered { lang_id: id, revision, }
        } else {
            Registered { lang_id: id, revision: self.revision(), }
        }
    }

    /// Installs the callback used to load, on demand, languages that are `embed`ded but not
    /// registered yet. It is invoked **once per language name** (results, including failures, are
    /// remembered). Bumps the revision.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.set_loader(|name| {
    ///     if name == "dummy" {
    ///         Some("[meta]\nname = \"dummy\"\n[states.root]\nrules = []".to_string())
    ///     } else {
    ///         None
    ///     }
    /// });
    /// ```
    pub fn set_loader(&self, loader: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> u64 {
        *self.loader.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(loader));
        self.retry_unavailable()
    }

    /// Forgets which languages the loader failed to provide so it is asked again, and bumps the
    /// revision so documents are re-lexed. Call it when a language that was missing (e.g. being
    /// loaded asynchronously) becomes available.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let rev = registry.retry_unavailable();
    /// assert!(rev > 1);
    /// ```
    pub fn retry_unavailable(&self) -> u64 {
        self.write_langs().unavailable.clear();
        self.bump_revision()
    }

    /// Id of the registered language called `name` (or using the file extension `name`).
    /// Never invokes the loader.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("rust", "[meta]\nname = \"rust\"\nextensions = [\"rs\"]\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(registry.lang_id("rust"), Some(0));
    /// assert_eq!(registry.lang_id("rs"), Some(0));
    /// assert_eq!(registry.lang_id("unknown"), None);
    /// ```
    pub fn lang_id(&self, name: &str) -> Option<LangId> {
        let key = name.trim().to_ascii_lowercase();
        let langs = self.read_langs();
        langs.by_name.get(&key).or_else(|| langs.by_ext.get(&key)).copied()
    }

    /// Like [`LanguageRegistry::lang_id`] but falls back to the loader (once per name).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.set_loader(|name| {
    ///     if name == "py" {
    ///         Some("[meta]\nname = \"python\"\nextensions = [\"py\"]\n[states.root]\nrules = []".into())
    ///     } else {
    ///         None
    ///     }
    /// });
    /// assert!(registry.resolve("py").is_some());
    /// ```
    pub fn resolve(&self, name: &str) -> Option<LangId> {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        {
            let langs = self.read_langs();
            if let Some(&id) = langs.by_name.get(&key).or_else(|| langs.by_ext.get(&key)) {
                return Some(id);
            }
            if langs.unavailable.contains(&key) {
                return None;
            }
        }
        let loader = self.loader.read().unwrap_or_else(|e| e.into_inner()).clone()?;
        // No lock is held while the callback runs (it may do IO or call back into the registry).
        let loaded = loader(name.trim()).and_then(|source| lang::compile(&source).ok());
        match loaded {
            // A language that was not known cannot have influenced already cached lines.
            Some(lang) => Some(self.insert(&key, lang, false).lang_id),
            None => {
                self.write_langs().unavailable.insert(key);
                None
            }
        }
    }

    /// Compiled language with the given id.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("toy", "[meta]\nname = \"toy\"\n[states.root]\nrules = []").unwrap();
    /// assert!(registry.language(0).is_some());
    /// ```
    pub fn language(&self, id: LangId) -> Option<Arc<CompiledLanguage>> {
        self.read_langs().by_id.get(id as usize).cloned()
    }

    /// Names of the registered languages, in registration order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("rust", "[meta]\nname = \"rust\"\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(registry.language_names(), vec!["rust"]);
    /// ```
    pub fn language_names(&self) -> Vec<String> {
        self.read_langs().names.clone()
    }

    /// Canonical name of the registered language for a [`LangId`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// let reg = registry.register("rust", "[meta]\nname = \"rust\"\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(registry.lang_name(reg.lang_id), Some("rust".to_string()));
    /// ```
    pub fn lang_name(&self, id: LangId) -> Option<String> {
        self.read_langs().names.get(id as usize).cloned()
    }

    // ---------------------------------------------------------------------------------------
    // Introspection
    // ---------------------------------------------------------------------------------------

    /// The stack a [`LexState`] stands for (outermost frame first).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    /// use texera::LexState;
    ///
    /// let registry = LanguageRegistry::new();
    /// let stack = registry.stack_of(LexState::BASE, 0);
    /// assert_eq!(stack.len(), 1);
    /// ```
    pub fn stack_of(&self, state: LexState, lang: LangId) -> Vec<StateFrame> {
        let base = StateFrame { lang_id: lang, state_id: lang::ROOT_STATE_ID, embed_end_id: 0, };
        self.decode(state, base).to_vec()
    }

    /// Readable form of a stack: `["markdown::root", "rust::fn_body"]`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    /// use texera::LexState;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// let desc = registry.describe(LexState::BASE, 0);
    /// assert_eq!(desc, vec!["test::root"]);
    /// ```
    pub fn describe(&self, state: LexState, lang: LangId) -> Vec<String> {
        self.stack_of(state, lang)
            .into_iter()
            .map(|f| {
                if f.lang_id == PLAIN_LANG {
                    return "<plain>".to_string();
                }
                let langs = self.read_langs();
                let name = langs.names.get(f.lang_id as usize).map_or("?", String::as_str);
                let state = langs
                    .by_id
                    .get(f.lang_id as usize)
                    .and_then(|l| l.state_name(f.state_id).map(str::to_string))
                    .unwrap_or_else(|| "?".to_string());
                format!("{name}::{state}")
            })
            .collect()
    }
}
