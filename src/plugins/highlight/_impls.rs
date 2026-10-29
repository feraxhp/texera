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

//! View and highlight trait integration for HighlightPlugin.

use std::ops::Range;
use std::sync::Arc;

use ratatui::style::Style;
use texera_core::State;

use super::syntax_cache::SyntaxField;
use super::plugin::HighlightPlugin;
use super::registry::LexState;
use super::registry::LangId;
use super::theme::ThemeMap;
use crate::ViewPlugin;
use crate::ViewUpdate;
use crate::Highlight;

impl HighlightPlugin {
    /// Resolves the style of every token of `line`, returning **char** ranges.
    pub(crate) fn styled_tokens(
        &self,
        host_lang: LangId,
        host_theme: Option<&ThemeMap>,
        line: &str,
        entry: LexState,
    ) -> Vec<(Range<usize>, Style)> {
        let mut state = entry;
        let token_ranges = self.registry().tokenize_line_stateful(host_lang, line, &mut state);
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
        cache.advance(state.doc(), self.registry(), lang, through, self.parse_budget);
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
            .and_then(|cache| cache.entry_state(line_index, self.registry().rules_id(lang)))
            .unwrap_or(fallback);
        let host_theme = self.theme_of(name);
        self.styled_tokens(lang, host_theme.as_deref(), line, entry)
    }
}
