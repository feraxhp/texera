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

//! Grammar compilation from raw TOML state tables into executable automatons.

use std::collections::HashMap;
use std::sync::Arc;

use regex_automata::meta::Regex;
use regex_automata::MatchKind;

use super::types::CompiledLanguage;
use super::types::DOC_START_STATE;
use super::types::CompiledState;
use super::types::CompiledRule;
use super::types::SyntaxError;
use super::types::ROOT_STATE;
use super::types::EmbedName;
use super::types::EmbedSpec;
use super::types::TokenName;
use super::raw::parse_state;
use super::types::EmbedEnd;
use super::raw::parse_meta;
use super::raw::StateDef;
use super::types::Action;
use super::raw::RuleDef;
use super::raw::Item;

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
                out.push(FlatRule { origin: name.to_string(), index, def: rule.clone(), });
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
                Action::Embed(EmbedSpec { name, end, end_id: 0, })
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

    Ok(CompiledLanguage { name, extensions, states, warnings, })
}
