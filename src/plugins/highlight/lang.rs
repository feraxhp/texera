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

//! TOML language definitions and their compilation into a state machine.
//!
//! A language file is declarative:
//!
//! ```toml
//! [meta]
//! name = "json"
//! extensions = ["json"]
//!
//! [states.root]
//! include = ["value"]               # splice the rules of other states (flattened at compile time)
//! pop_on_newline = false            # pop automatically when a line ends in this state
//! default_token = "string"          # label for text no rule matches
//! rules = [
//!     { match = '\{', token = "punctuation.bracket", push = "object" },
//!     { include = "comments" },     # an include placed *inside* `rules` keeps its position
//! ]
//! ```
//!
//! Rules are ordered, the first one that matches wins. A rule may carry **one** transition:
//! `push`, `pop`, `set` or `embed` (+ `embed_end`).
//!
//! # Ordering of `include`
//!
//! A TOML table cannot repeat a key, so the position of the includes is expressed in two ways:
//!
//! * a state-level `include = [..]` is spliced **before** the state's own rules if it is written
//!   before `rules` in the file, and **after** them otherwise (document order is preserved);
//! * `{ include = "state" }` inside `rules` is spliced exactly where it is written.
//!
//! # `lookahead`
//!
//! `lookahead` is a regex that must match right after `match` without being consumed. It is
//! relative to the end of the match, so a leading `^` is accepted and ignored
//! (`lookahead = '^\s*\('` is the same as `'\s*\('`).
//!
//! # `first_line`
//!
//! A rule directly in `root` can have `first_line = true`. It is active strictly on line 0 of the
//! document (e.g. shebangs, Markdown YAML frontmatter). The compiler synthesizes an internal
//! `doc_start` state containing these rules followed by `root`'s rules, while `root` itself is
//! compiled without them for zero runtime overhead on the rest of the document.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use regex_automata::meta::Regex;
use regex_automata::MatchKind;
use serde::Deserialize;

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
        Self { message: message.into() }
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

// -------------------------------------------------------------------------------------------------
// Raw (deserialized) definition
// -------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleDef {
    #[serde(rename = "match")]
    pattern: Option<String>,
    lookahead: Option<String>,
    token: Option<String>,
    push: Option<String>,
    pop: Option<bool>,
    set: Option<String>,
    embed: Option<String>,
    embed_end: Option<String>,
    embed_end_token: Option<String>,
    include: Option<String>,
    first_line: Option<bool>,
}

#[derive(Clone, Debug)]
enum Item {
    Include(String),
    Rule(RuleDef),
}

#[derive(Debug, Default)]
struct StateDef {
    items: Vec<Item>,
    pop_on_newline: bool,
    default_token: Option<String>,
}

fn parse_meta(doc: &toml::Table) -> Result<(String, Vec<String>), SyntaxError> {
    let Some(meta) = doc.get("meta") else {
        return Ok((String::new(), Vec::new()));
    };
    let meta = meta
        .as_table()
        .ok_or_else(|| SyntaxError::new("`[meta]` must be a table"))?;
    let mut name = String::new();
    let mut extensions = Vec::new();
    for (key, value) in meta {
        match key.as_str() {
            "name" => {
                name = value
                    .as_str()
                    .ok_or_else(|| SyntaxError::new("`meta.name` must be a string"))?
                    .to_string();
            }
            "extensions" => {
                let list = value
                    .as_array()
                    .ok_or_else(|| SyntaxError::new("`meta.extensions` must be an array of strings"))?;
                for ext in list {
                    let ext = ext
                        .as_str()
                        .ok_or_else(|| SyntaxError::new("`meta.extensions` must be an array of strings"))?;
                    extensions.push(ext.trim_start_matches('.').to_ascii_lowercase());
                }
            }
            other => return Err(SyntaxError::new(format!("unknown key `meta.{other}`"))),
        }
    }
    Ok((name, extensions))
}

fn string_list(state: &str, key: &str, value: &toml::Value) -> Result<Vec<String>, SyntaxError> {
    let err = || SyntaxError::new(format!("state `{state}`: `{key}` must be an array of state names"));
    let list = value.as_array().ok_or_else(err)?;
    list.iter()
        .map(|v| v.as_str().map(str::to_string).ok_or_else(err))
        .collect()
}

fn parse_state(name: &str, value: &toml::Value) -> Result<StateDef, SyntaxError> {
    let table = value
        .as_table()
        .ok_or_else(|| SyntaxError::new(format!("`states.{name}` must be a table")))?;
    let mut def = StateDef::default();
    for (key, value) in table {
        match key.as_str() {
            "include" => def.items.extend(string_list(name, "include", value)?.into_iter().map(Item::Include)),
            "rules" => {
                let rules = value
                    .as_array()
                    .ok_or_else(|| SyntaxError::new(format!("state `{name}`: `rules` must be an array")))?;
                for (idx, raw) in rules.iter().enumerate() {
                    let rule: RuleDef = raw
                        .clone()
                        .try_into()
                        .map_err(|e| SyntaxError::new(format!("state `{name}` rule #{}: {e}", idx + 1)))?;
                    if name != ROOT_STATE && rule.first_line.is_some() {
                        return Err(SyntaxError::new(format!(
                            "state `{name}` rule #{}: `first_line` is only allowed in `root`",
                            idx + 1
                        )));
                    }
                    if let Some(target) = rule.include.clone() {
                        if rule.pattern.is_some() {
                            return Err(SyntaxError::new(format!(
                                "state `{name}` rule #{}: `include` cannot be combined with `match`",
                                idx + 1
                            )));
                        }
                        if rule.first_line.is_some() {
                            return Err(SyntaxError::new(format!(
                                "state `{name}` rule #{}: `include` cannot be combined with `first_line`",
                                idx + 1
                            )));
                        }
                        def.items.push(Item::Include(target));
                    } else {
                        def.items.push(Item::Rule(rule));
                    }
                }
            }
            "pop_on_newline" => {
                def.pop_on_newline = value
                    .as_bool()
                    .ok_or_else(|| SyntaxError::new(format!("state `{name}`: `pop_on_newline` must be a bool")))?;
            }
            "default_token" => {
                def.default_token = Some(
                    value
                        .as_str()
                        .ok_or_else(|| SyntaxError::new(format!("state `{name}`: `default_token` must be a string")))?
                        .to_string(),
                );
            }
            other => return Err(SyntaxError::new(format!("state `{name}`: unknown key `{other}`"))),
        }
    }
    Ok(def)
}

// -------------------------------------------------------------------------------------------------
// Compiled representation
// -------------------------------------------------------------------------------------------------

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
    /// use texera::plugins::highlight::lang::{compile, ROOT_STATE_ID};
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

// -------------------------------------------------------------------------------------------------
// Compilation
// -------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct FlatRule {
    origin: String,
    index: usize,
    def: RuleDef,
}

fn flatten(
    name: &str,
    defs: &HashMap<String, StateDef>,
    visiting: &mut Vec<String>,
    out: &mut Vec<FlatRule>,
) -> Result<(), SyntaxError> {
    if let Some(pos) = visiting.iter().position(|s| s == name) {
        let mut cycle: Vec<&str> = visiting[pos..].iter().map(String::as_str).collect();
        cycle.push(name);
        return Err(SyntaxError::new(format!("include cycle: {}", cycle.join(" -> "))));
    }
    let def = defs.get(name).ok_or_else(|| {
        let from = visiting.last().map(String::as_str).unwrap_or("?");
        SyntaxError::new(format!("state `{from}` includes unknown state `{name}`"))
    })?;
    visiting.push(name.to_string());
    let mut index = 0;
    for item in &def.items {
        match item {
            Item::Include(target) => flatten(target, defs, visiting, out)?,
            Item::Rule(rule) => {
                index += 1;
                out.push(FlatRule { origin: name.to_string(), index, def: rule.clone() });
            }
        }
    }
    visiting.pop();
    Ok(())
}

fn min_len(pattern: &str) -> Result<Option<usize>, String> {
    regex_syntax::ParserBuilder::new()
        .build()
        .parse(pattern)
        .map(|hir| hir.properties().minimum_len())
        .map_err(|e| e.to_string())
}

fn build(patterns: &[String]) -> Result<Regex, String> {
    Regex::builder()
        .configure(Regex::config().match_kind(MatchKind::LeftmostFirst))
        .build_many(patterns)
        .map_err(|e| e.to_string())
}

fn build_one(pattern: &str) -> Result<Regex, String> {
    build(std::slice::from_ref(&pattern.to_string()))
}

struct Interner(HashMap<String, TokenName>);

impl Interner {
    fn get(&mut self, token: &str) -> TokenName {
        self.0
            .entry(token.to_string())
            .or_insert_with(|| Arc::from(token))
            .clone()
    }
}

/// Parses and compiles a TOML language definition into a state machine.
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
/// ```
pub fn compile(source: &str) -> Result<CompiledLanguage, SyntaxError> {
    let doc: toml::Table = source
        .parse()
        .map_err(|e: toml::de::Error| SyntaxError::new(format!("invalid TOML: {e}")))?;

    for key in doc.keys() {
        if key != "meta" && key != "states" {
            return Err(SyntaxError::new(format!("unknown top level key `{key}`")));
        }
    }

    let (name, extensions) = parse_meta(&doc)?;

    let states_table = doc
        .get("states")
        .and_then(|v| v.as_table())
        .ok_or_else(|| SyntaxError::new("missing `[states]` table"))?;

    if states_table.contains_key(DOC_START_STATE) {
        return Err(SyntaxError::new(format!(
            "state `{DOC_START_STATE}` is reserved for the document start state"
        )));
    }

    let mut defs: HashMap<String, StateDef> = HashMap::new();
    // `root` is always state 0, `doc_start` is always state 1.
    let mut order: Vec<String> = vec![ROOT_STATE.to_string(), DOC_START_STATE.to_string()];
    for (state_name, value) in states_table {
        defs.insert(state_name.clone(), parse_state(state_name, value)?);
        if state_name != ROOT_STATE {
            order.push(state_name.clone());
        }
    }
    if !defs.contains_key(ROOT_STATE) {
        return Err(SyntaxError::new("missing `[states.root]`"));
    }
    if order.len() >= u16::MAX as usize {
        return Err(SyntaxError::new("too many states"));
    }
    let ids: HashMap<&str, u16> = order.iter().enumerate().map(|(i, n)| (n.as_str(), i as u16)).collect();

    let mut interner = Interner(HashMap::new());
    let mut warnings = Vec::new();
    let mut embed_ends: HashMap<(String, Option<String>), Arc<EmbedEnd>> = HashMap::new();
    let mut states = Vec::with_capacity(order.len());

    let mut root_flat = Vec::new();
    flatten(ROOT_STATE, &defs, &mut Vec::new(), &mut root_flat)?;
    let root_def = &defs[ROOT_STATE];

    for state_name in &order {
        let (flat, pop_on_newline, default_token) = if state_name == DOC_START_STATE {
            let mut rules: Vec<FlatRule> = root_flat
                .iter()
                .filter(|r| r.def.first_line == Some(true))
                .cloned()
                .collect();
            rules.extend(
                root_flat
                    .iter()
                    .filter(|r| r.def.first_line != Some(true))
                    .cloned(),
            );
            (rules, root_def.pop_on_newline, root_def.default_token.as_deref())
        } else if state_name == ROOT_STATE {
            let rules: Vec<FlatRule> = root_flat
                .iter()
                .filter(|r| r.def.first_line != Some(true))
                .cloned()
                .collect();
            (rules, root_def.pop_on_newline, root_def.default_token.as_deref())
        } else {
            let mut state_flat = Vec::new();
            flatten(state_name, &defs, &mut Vec::new(), &mut state_flat)?;
            let rules: Vec<FlatRule> = state_flat
                .into_iter()
                .filter(|r| r.def.first_line != Some(true))
                .collect();
            let def = &defs[state_name];
            (rules, def.pop_on_newline, def.default_token.as_deref())
        };

        let mut rules = Vec::with_capacity(flat.len());
        let mut patterns = Vec::with_capacity(flat.len());

        for FlatRule { origin, index, def: rule } in flat {
            let at = if origin == *state_name {
                format!("state `{state_name}` rule #{index}")
            } else {
                format!("state `{state_name}` (rule #{index} of `{origin}`)")
            };
            let err = |msg: String| SyntaxError::new(format!("{at}: {msg}"));

            let pattern = rule
                .pattern
                .as_deref()
                .ok_or_else(|| err("missing `match`".into()))?;
            let pattern_min = min_len(pattern).map_err(|e| err(format!("invalid `match` regex: {e}")))?;

            let lookahead = rule.lookahead.as_deref().map(|l| l.strip_prefix('^').unwrap_or(l));
            if let Some(l) = lookahead {
                min_len(l).map_err(|e| err(format!("invalid `lookahead` regex: {e}")))?;
            }

            // Transition.
            let mut actions = 0;
            actions += rule.push.is_some() as u8;
            actions += (rule.pop == Some(true)) as u8;
            actions += rule.set.is_some() as u8;
            actions += rule.embed.is_some() as u8;
            if actions > 1 {
                return Err(err("`push`, `pop`, `set` and `embed` are mutually exclusive".into()));
            }
            if rule.embed.is_none() && (rule.embed_end.is_some() || rule.embed_end_token.is_some()) {
                return Err(err("`embed_end` requires `embed`".into()));
            }
            let resolve = |target: &str| -> Result<u16, SyntaxError> {
                if target == DOC_START_STATE {
                    return Err(err("transitions cannot target the internal `doc_start` state".into()));
                }
                ids.get(target)
                    .copied()
                    .ok_or_else(|| err(format!("unknown state `{target}`")))
            };

            let mut needs_main = lookahead.is_some();
            let action = if let Some(t) = &rule.push {
                Action::Push(resolve(t)?)
            } else if rule.pop == Some(true) {
                Action::Pop
            } else if let Some(t) = &rule.set {
                Action::Set(resolve(t)?)
            } else if let Some(template) = &rule.embed {
                let end_pattern = rule
                    .embed_end
                    .as_deref()
                    .ok_or_else(|| err("`embed` requires `embed_end`".into()))?;
                let name = match template.strip_prefix('$').map(str::parse::<usize>) {
                    Some(Ok(n)) => {
                        needs_main = true;
                        EmbedName::Group(n)
                    }
                    Some(Err(_)) => return Err(err(format!("invalid embed name `{template}`"))),
                    None => EmbedName::Fixed(template.clone()),
                };
                let end_token = rule.embed_end_token.clone().or_else(|| rule.token.clone());
                let end = match embed_ends.get(&(end_pattern.to_string(), end_token.clone())) {
                    Some(end) => end.clone(),
                    None => {
                        min_len(end_pattern).map_err(|e| err(format!("invalid `embed_end` regex: {e}")))?;
                        let regex = build_one(&format!("(?:{end_pattern})"))
                            .map_err(|e| err(format!("invalid `embed_end` regex: {e}")))?;
                        let end = Arc::new(EmbedEnd {
                            pattern: end_pattern.to_string(),
                            regex,
                            token: end_token.as_deref().map(|t| interner.get(t)),
                        });
                        embed_ends.insert((end_pattern.to_string(), end_token), end.clone());
                        end
                    }
                };
                Action::Embed(EmbedSpec { name, end, end_id: 0 })
            } else {
                Action::None
            };

            let is_duplicate_warning = state_name == DOC_START_STATE && rule.first_line != Some(true);
            if !is_duplicate_warning && matches!(action, Action::None) && pattern_min == Some(0) {
                warnings.push(format!(
                    "{at}: `match` can match zero bytes and the rule has no transition; \
                     it will shadow the following rules and the cursor will be force-advanced"
                ));
            }

            let combined = match lookahead {
                Some(l) => format!("(?:{pattern})(?:{l})"),
                None => format!("(?:{pattern})"),
            };
            let main = if needs_main {
                let main_pattern = match lookahead {
                    Some(l) => format!("((?:{pattern}))(?:{l})"),
                    None => format!("((?:{pattern}))"),
                };
                Some(build_one(&main_pattern).map_err(|e| err(format!("cannot compile rule: {e}")))?)
            } else {
                None
            };

            patterns.push(combined);
            rules.push(CompiledRule {
                token: rule.token.as_deref().map(|t| interner.get(t)),
                action,
                main,
            });
        }

        let regex = if patterns.is_empty() {
            None
        } else {
            Some(build(&patterns).map_err(|e| {
                SyntaxError::new(format!("state `{state_name}`: cannot compile rules: {e}"))
            })?)
        };

        states.push(CompiledState {
            name: state_name.clone(),
            regex,
            rules,
            pop_on_newline,
            default_token: default_token.map(|t| interner.get(t)),
        });
    }

    Ok(CompiledLanguage { name, extensions, states, warnings })
}
