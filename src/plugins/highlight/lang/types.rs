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

//! AST, compiled states, and error types for declarative TOML syntax definitions.

use std::sync::Arc;
use std::fmt;

use regex_automata::meta::Regex;

/// Name of a token as emitted by a language (`"keyword.control"`, `"string"`, ...).
pub type TokenName = Arc<str>;

/// Name of the initial state of every language.
pub const ROOT_STATE: &str = "root";
/// Numeric index of `root` (always 0).
pub const ROOT_STATE_ID: u16 = 0;

/// Name of the internal document-start state generated for line 0.
pub const DOC_START_STATE: &str = "doc_start";
/// Numeric index of `doc_start` (always 1).
pub const DOC_START_STATE_ID: u16 = 1;

/// Error produced while loading or compiling a language definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    message: String,
}

impl SyntaxError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), }
    }

    /// Human readable description of the problem.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let err = compile("invalid toml [[[").unwrap_err();
    /// assert!(!err.message().is_empty());
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// Name of the embedded language of an `embed` rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EmbedName {
    /// Fixed name (`embed = "rust"`).
    Fixed(String),
    /// Capture group of the rule's `match` (`embed = "$1"`).
    Group(usize),
}

/// Closing delimiter of an embedded block, shared by every rule that uses the same pattern.
pub(crate) struct EmbedEnd {
    pub pattern: String,
    pub regex: Regex,
    pub token: Option<TokenName>,
}

impl fmt::Debug for EmbedEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbedEnd").field("pattern", &self.pattern).finish()
    }
}

#[derive(Debug)]
pub(crate) struct EmbedSpec {
    pub name: EmbedName,
    pub end: Arc<EmbedEnd>,
    /// Index (+1) of `end` inside the registry table. Assigned on registration.
    pub end_id: u16,
}

#[derive(Debug)]
pub(crate) enum Action {
    None,
    Push(u16),
    Pop,
    Set(u16),
    Embed(EmbedSpec),
}

pub(crate) struct CompiledRule {
    pub token: Option<TokenName>,
    pub action: Action,
    /// Individual anchored regex `((?:match))(?:lookahead)`, only for rules that need the exact end
    /// of `match` (lookahead) or its capture groups (`embed = "$n"`). Group 1 is `match`.
    pub main: Option<Regex>,
}

impl fmt::Debug for CompiledRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompiledRule")
            .field("token", &self.token)
            .field("action", &self.action)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct CompiledState {
    pub name: String,
    /// Multi-pattern automaton of the (flattened) rules; `None` if the state has no rules.
    pub regex: Option<Regex>,
    pub rules: Vec<CompiledRule>,
    pub pop_on_newline: bool,
    pub default_token: Option<TokenName>,
}

/// A language definition compiled into a state machine ready to lex lines.
///
/// # Examples
///
/// ```rust
/// use texera::plugins::highlight::lang::compile;
///
/// let toml = r#"
/// [meta]
/// name = "toy"
/// extensions = ["toy"]
///
/// [states.root]
/// rules = [
///     { match = '\b\d+\b', token = "number" }
/// ]
/// "#;
/// let lang = compile(toml).unwrap();
/// assert_eq!(lang.name(), "toy");
/// assert_eq!(lang.extensions(), &["toy"]);
/// ```
#[derive(Debug)]
pub struct CompiledLanguage {
    pub(crate) name: String,
    pub(crate) extensions: Vec<String>,
    /// `states[0]` is always `root`, `states[1]` is always `doc_start`.
    pub(crate) states: Vec<CompiledState>,
    pub(crate) warnings: Vec<String>,
}

impl CompiledLanguage {
    /// Name declared in `[meta]` (empty if missing).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let lang = compile("[meta]\nname = \"json\"\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(lang.name(), "json");
    /// ```
    pub fn name(&self) -> &str {
        &self.name
    }

    /// File extensions declared in `[meta]` (lowercase, without dot).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let lang = compile("[meta]\nname = \"json\"\nextensions = [\"json\"]\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(lang.extensions(), &["json"]);
    /// ```
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// Number of states of the machine (includes `root`, `doc_start` and all user-defined states).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let lang = compile("[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// assert!(lang.state_count() >= 2);
    /// ```
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Name of the state with the given id.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::ROOT_STATE_ID;
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let lang = compile("[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// assert_eq!(lang.state_name(ROOT_STATE_ID), Some("root"));
    /// ```
    pub fn state_name(&self, id: u16) -> Option<&str> {
        self.states.get(id as usize).map(|s| s.name.as_str())
    }

    /// Non fatal problems found while compiling (e.g. rules that can match zero bytes).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::plugins::highlight::lang::compile;
    ///
    /// let lang = compile("[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// assert!(lang.warnings().is_empty());
    /// ```
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub(crate) fn embed_specs_mut(&mut self) -> impl Iterator<Item = &mut EmbedSpec> {
        self.states
            .iter_mut()
            .flat_map(|s| s.rules.iter_mut())
            .filter_map(|r| match &mut r.action {
                Action::Embed(spec) => Some(spec),
                _ => None,
            })
    }
}
