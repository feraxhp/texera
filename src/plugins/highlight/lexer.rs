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

//! Incremental, stack-based line lexer and token stream emission.

use std::ops::Range;
use std::sync::Arc;

use regex_automata::Anchored;
use regex_automata::Input;
use smallvec::smallvec;

use super::registry::LanguageRegistry;
use super::lang::CompiledLanguage;
use super::registry::ScopedToken;
use super::registry::PLAIN_LANG;
use super::pool::StateFrame;
use super::registry::LangId;
use super::lang::EmbedName;
use super::lang::TokenName;
use super::pool::MAX_DEPTH;
use super::lang::EmbedEnd;
use super::pool::LexState;
use super::lang::Action;
use super::pool::Stack;
use super::lang;

/// Maximum number of consecutive zero-width transitions at the same byte position.
const MAX_ZERO_WIDTH_STEPS: u32 = 64;

/// Receives the tokens produced by the lexer.
pub(crate) trait Sink {
    fn emit(&mut self, range: Range<usize>, lang_id: LangId, token: &TokenName);
}

/// Used to compute only the exit state of a line.
pub(crate) struct NullSink;

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
    /// use texera::LanguageRegistry;
    /// use texera::LexState;
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
    /// use texera::LanguageRegistry;
    /// use texera::LexState;
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
    /// use texera::LanguageRegistry;
    /// use texera::LexState;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("test", "[meta]\nname = \"test\"\n[states.root]\nrules = []").unwrap();
    /// let exit = registry.scan_line(0, "hello", LexState::BASE);
    /// assert_eq!(exit, LexState::BASE);
    /// ```
    pub fn scan_line(&self, lang: LangId, line: &str, entry: LexState) -> LexState {
        self.lex(lang, line, entry, &mut NullSink)
    }

    pub(crate) fn decode(&self, state: LexState, base: StateFrame) -> Stack {
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

    pub(crate) fn encode(&self, stack: &Stack, base: StateFrame) -> LexState {
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

    pub(crate) fn find_embed(&self, stack: &Stack) -> (Option<usize>, Option<Arc<EmbedEnd>>) {
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
    pub(crate) fn lex<S: Sink>(&self, lang_id: LangId, line: &str, entry: LexState, sink: &mut S) -> LexState {
        let Some(base_lang) = self.language(lang_id) else {
            return LexState::BASE;
        };
        let base = StateFrame { lang_id, state_id: lang::ROOT_STATE_ID, embed_end_id: 0, };
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
                        stack.push(StateFrame { lang_id: top.lang_id, state_id: *target, embed_end_id: 0, });
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
                        stack.push(StateFrame { lang_id: embedded, state_id: 0, embed_end_id: spec.end_id, });
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
