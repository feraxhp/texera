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
//!
//! # Safety nets
//!
//! * a rule that matches zero bytes can never freeze the lexer (the cursor is force-advanced);
//! * `pop` never removes the base frame (nor the root frame of an embedded language);
//! * the stack is capped to [`MAX_DEPTH`] frames, extra `push`es are ignored;
//! * the state pool is capped to [`MAX_POOL_STATES`]; on overflow it is cleared and the registry
//!   revision is bumped so every cache derived from it is rebuilt;
//! * the delimiter of an `embed` block always wins over whatever the embedded language is doing
//!   (single pass lexers cannot look ahead; TextMate has the same limit).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use regex_automata::{Anchored, Input};
use smallvec::{SmallVec, smallvec};

use super::lang::{self, Action, CompiledLanguage, EmbedEnd, EmbedName, SyntaxError, TokenName};

/// Identifier of a language inside a [`LanguageRegistry`].
pub type LangId = u16;

/// A token with UTF-8 byte range, generating language identifier, and token name.
pub type ScopedToken = (Range<usize>, LangId, TokenName);

/// Pseudo language of an `embed` block whose language is not available: nothing is highlighted
/// until the block's delimiter is found.
pub const PLAIN_LANG: LangId = u16::MAX;

/// Maximum depth of the state stack. Extra `push`es are silently ignored.
pub const MAX_DEPTH: usize = 256;

/// Maximum number of distinct stacks kept by the pool before it is flushed.
pub const MAX_POOL_STATES: usize = 65_536;

/// Maximum number of consecutive zero-width transitions at the same byte position.
const MAX_ZERO_WIDTH_STEPS: u32 = 64;

/// Lexical state carried from the end of one line to the start of the next.
///
/// It is the only information needed to lex a line independently of the previous ones, which is
/// what makes per-line caching (see [`super::SyntaxCache`]) possible. Internally it is an index
/// into the [`LanguageRegistry`]'s state pool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LexState(pub u32);

impl LexState {
    /// The base stack `[(active language, root)]`: nothing is open.
    pub const BASE: LexState = LexState(0);

    /// The start-of-document stack `[(active language, doc_start)]`: entry state for line 0.
    pub const DOC_START: LexState = LexState(1);

    /// Whether this is the base state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LexState;
    ///
    /// assert!(LexState::BASE.is_base());
    /// assert!(!LexState::DOC_START.is_base());
    /// ```
    pub fn is_base(self) -> bool {
        self == Self::BASE
    }

    /// Whether this is the document start state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LexState;
    ///
    /// assert!(LexState::DOC_START.is_doc_start());
    /// assert!(!LexState::BASE.is_doc_start());
    /// ```
    pub fn is_doc_start(self) -> bool {
        self == Self::DOC_START
    }
}

/// One level of the lexer's stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateFrame {
    /// Identifier of the language active in this stack frame.
    pub lang_id: u16,
    /// Numeric identifier of the state within the language (0 for root, 1 for doc_start).
    pub state_id: u16,
    /// Non zero for the root frame of an `embed`ded language: index (+1) of its closing delimiter.
    pub embed_end_id: u16,
}

type Stack = SmallVec<[StateFrame; 8]>;

/// Interner `stack <-> u32`. Indices 0 (BASE) and 1 (DOC_START) are reserved.
#[derive(Debug)]
struct StatePool {
    stacks: Vec<Stack>,
    index: HashMap<Stack, u32>,
}

impl StatePool {
    fn new() -> Self {
        Self { stacks: vec![Stack::new(), Stack::new()], index: HashMap::new() }
    }

    fn clear(&mut self) {
        self.stacks.truncate(2);
        self.index.clear();
    }

    /// `None` when the pool is full.
    fn intern(&mut self, stack: &Stack) -> Option<u32> {
        if let Some(&id) = self.index.get(stack) {
            return Some(id);
        }
        if self.stacks.len() >= MAX_POOL_STATES {
            return None;
        }
        let id = self.stacks.len() as u32;
        self.stacks.push(stack.clone());
        self.index.insert(stack.clone(), id);
        Some(id)
    }
}

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
struct Langs {
    by_id: Vec<Arc<CompiledLanguage>>,
    names: Vec<String>,
    by_name: HashMap<String, LangId>,
    by_ext: HashMap<String, LangId>,
    /// Names the loader could not provide (so it is not asked again for every line).
    unavailable: HashSet<String>,
    embed_ends: Vec<Arc<EmbedEnd>>,
    embed_index: HashMap<(String, Option<String>), u16>,
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

/// Receives the tokens produced by the lexer.
trait Sink {
    fn emit(&mut self, range: Range<usize>, lang_id: LangId, token: &TokenName);
}

/// Used to compute only the exit state of a line.
struct NullSink;

impl Sink for NullSink {
    fn emit(&mut self, _: Range<usize>, _: LangId, _: &TokenName) {}
}

impl Sink for Vec<ScopedToken> {
    /// Adjacent ranges with the same token and language are merged (`h`,`o`,`l`,`a` -> `hola`).
    fn emit(&mut self, range: Range<usize>, lang_id: LangId, token: &TokenName) {
        if range.is_empty() {
            return;
        }
        if let Some((last, last_lang, last_token)) = self.last_mut() {
            if last.end == range.start
                && *last_lang == lang_id
                && (Arc::ptr_eq(last_token, token) || **last_token == **token)
            {
                last.end = range.end;
                return;
            }
        }
        self.push((range, lang_id, token.clone()));
    }
}

impl Sink for Vec<(Range<usize>, TokenName)> {
    /// Adjacent ranges with the same token are merged (`h`,`o`,`l`,`a` -> `hola`).
    fn emit(&mut self, range: Range<usize>, _lang_id: LangId, token: &TokenName) {
        if range.is_empty() {
            return;
        }
        if let Some((last, last_token)) = self.last_mut() {
            if last.end == range.start && (Arc::ptr_eq(last_token, token) || **last_token == **token) {
                last.end = range.end;
                return;
            }
        }
        self.push((range, token.clone()));
    }
}

fn char_len_at(line: &str, i: usize) -> usize {
    line[i..].chars().next().map_or(1, char::len_utf8)
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

    fn read_langs(&self) -> RwLockReadGuard<'_, Langs> {
        self.langs.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_langs(&self) -> RwLockWriteGuard<'_, Langs> {
        self.langs.write().unwrap_or_else(|e| e.into_inner())
    }

    fn pool(&self) -> MutexGuard<'_, StatePool> {
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

    fn bump_revision(&self) -> u64 {
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

    fn insert(&self, name: &str, mut lang: CompiledLanguage, replace: bool) -> Registered {
        let key = name.trim().to_ascii_lowercase();
        let mut langs = self.write_langs();
        if !replace {
            if let Some(&id) = langs.by_name.get(&key) {
                return Registered { lang_id: id, revision: self.revision() };
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
            Registered { lang_id: id, revision }
        } else {
            Registered { lang_id: id, revision: self.revision() }
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
    /// use texera::{LanguageRegistry, LexState};
    ///
    /// let registry = LanguageRegistry::new();
    /// let stack = registry.stack_of(LexState::BASE, 0);
    /// assert_eq!(stack.len(), 1);
    /// ```
    pub fn stack_of(&self, state: LexState, lang: LangId) -> Vec<StateFrame> {
        let base = StateFrame { lang_id: lang, state_id: lang::ROOT_STATE_ID, embed_end_id: 0 };
        self.decode(state, base).to_vec()
    }

    /// Readable form of a stack: `["markdown::root", "rust::fn_body"]`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, LexState};
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

    // ---------------------------------------------------------------------------------------
    // Lexing
    // ---------------------------------------------------------------------------------------

    /// Lexes `line` as if it were the first line of the document (starting in `DOC_START`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = [{ match = 'a', token = \"char\" }]").unwrap();
    /// let tokens = registry.tokenize_first_line(0, "a");
    /// assert_eq!(tokens.len(), 1);
    /// ```
    pub fn tokenize_first_line(&self, lang: LangId, line: &str) -> Vec<ScopedToken> {
        let mut state = LexState::DOC_START;
        self.tokenize_line_stateful(lang, line, &mut state)
    }

    /// Lexes `line` as if it started in the base state (`root`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::LanguageRegistry;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = [{ match = 'xyz', token = \"keyword\" }]").unwrap();
    /// let tokens = registry.tokenize_line(0, "xyz");
    /// assert_eq!(tokens.len(), 1);
    /// ```
    pub fn tokenize_line(&self, lang: LangId, line: &str) -> Vec<ScopedToken> {
        let mut state = LexState::BASE;
        self.tokenize_line_stateful(lang, line, &mut state)
    }

    /// Lexes `line` continuing from `state`, leaving in `state` the state at the end of the line.
    /// Returned ranges are UTF-8 byte offsets inside `line`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, LexState};
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = [{ match = '123', token = \"number\" }]").unwrap();
    /// let mut state = LexState::BASE;
    /// let tokens = registry.tokenize_line_stateful(0, "123", &mut state);
    /// assert_eq!(tokens.len(), 1);
    /// ```
    pub fn tokenize_line_stateful(
        &self,
        lang: LangId,
        line: &str,
        state: &mut LexState,
    ) -> Vec<ScopedToken> {
        let mut tokens = Vec::new();
        *state = self.lex(lang, line, *state, &mut tokens);
        tokens
    }

    /// Tokenizes `line` continuing from `state`, preserving the language that generated each token.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, LexState};
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = [{ match = '123', token = \"number\" }]").unwrap();
    /// let mut state = LexState::BASE;
    /// let tokens = registry.tokenize_line_scoped(0, "123", &mut state);
    /// assert_eq!(tokens.len(), 1);
    /// assert_eq!(tokens[0].1, 0);
    /// ```
    pub fn tokenize_line_scoped(
        &self,
        lang: LangId,
        line: &str,
        state: &mut LexState,
    ) -> Vec<ScopedToken> {
        self.tokenize_line_stateful(lang, line, state)
    }

    /// Exit state of `line` given its `entry` state, without building tokens. Runs the very same
    /// lexer as [`LanguageRegistry::tokenize_line_stateful`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, LexState};
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// let exit = registry.scan_line(0, "hello", LexState::BASE);
    /// assert_eq!(exit, LexState::BASE);
    /// ```
    pub fn scan_line(&self, lang: LangId, line: &str, entry: LexState) -> LexState {
        self.lex(lang, line, entry, &mut NullSink)
    }

    fn decode(&self, state: LexState, base: StateFrame) -> Stack {
        if state.is_base() {
            return smallvec![base];
        }
        if state.is_doc_start() {
            return smallvec![StateFrame {
                lang_id: base.lang_id,
                state_id: lang::DOC_START_STATE_ID,
                embed_end_id: 0,
            }];
        }
        match self.pool().stacks.get(state.0 as usize) {
            Some(stack) if !stack.is_empty() => stack.clone(),
            _ => smallvec![base],
        }
    }

    fn encode(&self, stack: &Stack, base: StateFrame) -> LexState {
        if stack.len() == 1 {
            if stack[0] == base {
                return LexState::BASE;
            }
            if stack[0].lang_id == base.lang_id
                && stack[0].state_id == lang::DOC_START_STATE_ID
                && stack[0].embed_end_id == 0
            {
                return LexState::DOC_START;
            }
        }
        let mut pool = self.pool();
        if let Some(id) = pool.intern(stack) {
            return LexState(id);
        }
        // Full: drop every interned stack and invalidate whatever was derived from them.
        pool.clear();
        let id = pool.intern(stack).expect("an empty pool has room");
        drop(pool);
        self.bump_revision();
        LexState(id)
    }

    fn find_embed(&self, stack: &Stack) -> (Option<usize>, Option<Arc<EmbedEnd>>) {
        let Some(idx) = stack.iter().rposition(|f| f.embed_end_id != 0) else {
            return (None, None);
        };
        let id = stack[idx].embed_end_id as usize;
        match self.read_langs().embed_ends.get(id - 1) {
            Some(end) => (Some(idx), Some(end.clone())),
            None => (None, None),
        }
    }

    /// Core of the engine. See the module docs for the guarantees it provides.
    fn lex<S: Sink>(&self, lang_id: LangId, line: &str, entry: LexState, sink: &mut S) -> LexState {
        let Some(base_lang) = self.language(lang_id) else {
            return LexState::BASE;
        };
        let base = StateFrame { lang_id, state_id: lang::ROOT_STATE_ID, embed_end_id: 0 };
        let mut stack = self.decode(entry, base);

        let mut cur_id = lang_id;
        let mut cur: Option<Arc<CompiledLanguage>> = Some(base_lang.clone());
        let (mut embed_idx, mut embed_end) = self.find_embed(&stack);

        let len = line.len();
        let mut i = 0usize;
        let mut zero_run = 0u32;

        while i < len {
            let top = *stack.last().expect("the stack is never empty");

            if top.lang_id != cur_id {
                cur_id = top.lang_id;
                cur = if cur_id == PLAIN_LANG { None } else { self.language(cur_id) };
                if cur.is_none() && cur_id != PLAIN_LANG {
                    // Dangling frame (language removed): fall back to the base stack.
                    stack.clear();
                    stack.push(base);
                    cur_id = lang_id;
                    cur = Some(base_lang.clone());
                    (embed_idx, embed_end) = (None, None);
                    continue;
                }
            }

            // The delimiter of an embedded block always wins.
            if let (Some(idx), Some(end)) = (embed_idx, &embed_end) {
                let input = Input::new(line).range(i..).anchored(Anchored::Yes);
                if let Some(m) = end.regex.search(&input) {
                    if let Some(token) = &end.token {
                        let parent_lang = if idx == 0 {
                            base.lang_id
                        } else {
                            stack[idx - 1].lang_id
                        };
                        sink.emit(i..m.end(), parent_lang, token);
                    }
                    if idx == 0 {
                        stack.clear();
                        stack.push(base);
                    } else {
                        stack.truncate(idx);
                    }
                    (embed_idx, embed_end) = self.find_embed(&stack);
                    i = m.end();
                    zero_run = 0;
                    continue;
                }
            }

            let Some(lang) = cur.as_ref() else {
                // Unknown embedded language: untagged text until the closing delimiter.
                let mut next = len;
                if let Some(end) = &embed_end {
                    let input = Input::new(line).range(i..);
                    if let Some(m) = end.regex.search(&input) {
                        next = m.start();
                    }
                }
                i = if next <= i { i + char_len_at(line, i) } else { next };
                continue;
            };

            let Some(state) = lang.states.get(top.state_id as usize) else {
                stack.clear();
                stack.push(base);
                (embed_idx, embed_end) = (None, None);
                continue;
            };

            let input = Input::new(line).range(i..).anchored(Anchored::Yes);
            let Some(m) = state.regex.as_ref().and_then(|re| re.search(&input)) else {
                // No rule matches here: jump to the next position where something could, labelling
                // the skipped text as one contiguous run with the state's `default_token`.
                let unanchored = Input::new(line).range(i..);
                let mut next = len;
                if let Some(m) = state.regex.as_ref().and_then(|re| re.search(&unanchored)) {
                    next = m.start();
                }
                if let Some(m) = embed_end.as_ref().and_then(|e| e.regex.search(&unanchored)) {
                    next = next.min(m.start());
                }
                if next <= i {
                    next = i + char_len_at(line, i);
                }
                if let Some(token) = &state.default_token {
                    sink.emit(i..next, top.lang_id, token);
                }
                i = next;
                zero_run = 0;
                continue;
            };

            let rule = &state.rules[m.pattern().as_usize()];

            // Exact end of `match` (without `lookahead`) and capture groups, in the same single
            // pass semantics: the rule's own automaton is the very one the state automaton chose.
            let mut end = m.end();
            let mut caps = None;
            if let Some(main) = &rule.main {
                let mut c = main.create_captures();
                main.search_captures(&input, &mut c);
                if c.is_match() {
                    if let Some(span) = c.get_group(1) {
                        end = span.end;
                    }
                    caps = Some(c);
                }
            }

            if end > i {
                if let Some(token) = &rule.token {
                    sink.emit(i..end, top.lang_id, token);
                }
            }

            let transitioned = !matches!(rule.action, Action::None);
            match &rule.action {
                Action::None => {}
                Action::Push(target) => {
                    if stack.len() < MAX_DEPTH {
                        stack.push(StateFrame { lang_id: top.lang_id, state_id: *target, embed_end_id: 0 });
                    }
                }
                Action::Pop => {
                    // The base frame and the root of an embedded language are immovable.
                    if stack.len() > 1 && top.embed_end_id == 0 {
                        stack.pop();
                    }
                }
                Action::Set(target) => {
                    if let Some(frame) = stack.last_mut() {
                        frame.state_id = *target;
                    }
                }
                Action::Embed(spec) => {
                    if stack.len() < MAX_DEPTH {
                        let name = match &spec.name {
                            EmbedName::Fixed(name) => Some(name.clone()),
                            EmbedName::Group(n) => caps
                                .as_ref()
                                .and_then(|c| c.get_group(n + 1))
                                .map(|span| line[span.range()].to_string()),
                        };
                        let embedded = name
                            .as_deref()
                            .and_then(|n| self.resolve(n))
                            .unwrap_or(PLAIN_LANG);
                        stack.push(StateFrame { lang_id: embedded, state_id: 0, embed_end_id: spec.end_id });
                        embed_idx = Some(stack.len() - 1);
                        embed_end = Some(spec.end.clone());
                    }
                }
            }

            if end == i {
                if transitioned && zero_run < MAX_ZERO_WIDTH_STEPS {
                    // Zero-width transition (e.g. `lookahead`-only rules): legal, but bounded.
                    zero_run += 1;
                    continue;
                }
                // Nothing consumed: force the cursor forward by one char, whatever the cause.
                let next = i + char_len_at(line, i);
                if let Some(token) = &state.default_token {
                    sink.emit(i..next, top.lang_id, token);
                }
                i = next;
                zero_run = 0;
                continue;
            }
            zero_run = 0;
            i = end;
        }

        // States flagged `pop_on_newline` do not survive the end of the line.
        while stack.len() > 1 {
            let top = *stack.last().expect("the stack is never empty");
            if top.embed_end_id != 0 || top.lang_id == PLAIN_LANG {
                break;
            }
            let lang = if top.lang_id == cur_id { cur.clone() } else { self.language(top.lang_id) };
            let pop = lang
                .and_then(|l| l.states.get(top.state_id as usize).map(|s| s.pop_on_newline))
                .unwrap_or(false);
            if !pop {
                break;
            }
            stack.pop();
        }

        // The document start state is strictly for line 0; degrade the base frame to root.
        // Even if line 0 pushed nested constructs (e.g. fn_body), the base frame stack[0]
        // must degrade from doc_start to root so that doc_start can never be popped back to.
        if !stack.is_empty() && stack[0].state_id == lang::DOC_START_STATE_ID {
            stack[0].state_id = lang::ROOT_STATE_ID;
        }

        self.encode(&stack, base)
    }
}
