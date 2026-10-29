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

//! Raw deserialized TOML representation of languages and states.

use serde::Deserialize;

use super::types::SyntaxError;
use super::types::ROOT_STATE;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleDef {
    #[serde(rename = "match")]
    pub(crate) pattern: Option<String>,
    pub(crate) lookahead: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) push: Option<String>,
    pub(crate) pop: Option<bool>,
    pub(crate) set: Option<String>,
    pub(crate) embed: Option<String>,
    pub(crate) embed_end: Option<String>,
    pub(crate) embed_end_token: Option<String>,
    pub(crate) include: Option<String>,
    pub(crate) first_line: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) enum Item {
    Include(String),
    Rule(RuleDef),
}

#[derive(Debug, Default)]
pub(crate) struct StateDef {
    pub(crate) items: Vec<Item>,
    pub(crate) pop_on_newline: bool,
    pub(crate) default_token: Option<String>,
}

pub(crate) fn parse_meta(doc: &toml::Table) -> Result<(String, Vec<String>), SyntaxError> {
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

pub(crate) fn parse_state(name: &str, value: &toml::Value) -> Result<StateDef, SyntaxError> {
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
