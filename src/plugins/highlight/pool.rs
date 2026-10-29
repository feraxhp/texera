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

//! State stacks and interned state pool for incremental lexing.

use std::collections::HashMap;

use smallvec::SmallVec;

/// Maximum depth of the state stack. Extra `push`es are silently ignored.
pub const MAX_DEPTH: usize = 256;

/// Maximum number of distinct stacks kept by the pool before it is flushed.
pub const MAX_POOL_STATES: usize = 65_536;

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

pub(crate) type Stack = SmallVec<[StateFrame; 8]>;

/// Interner `stack <-> u32`. Indices 0 (BASE) and 1 (DOC_START) are reserved.
#[derive(Debug)]
pub(crate) struct StatePool {
    pub(crate) stacks: Vec<Stack>,
    index: HashMap<Stack, u32>,
}

impl StatePool {
    pub(crate) fn new() -> Self {
        Self {
            stacks: vec![Stack::new(), Stack::new()],
            index: HashMap::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.stacks.truncate(2);
        self.index.clear();
    }

    /// `None` when the pool is full.
    pub(crate) fn intern(&mut self, stack: &Stack) -> Option<u32> {
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
