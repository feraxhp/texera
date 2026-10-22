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

//! Incremental, lazily-evaluated lexical-state cache tied to an editor [`State`].
//!
//! # Model
//!
//! Lexing a line only depends on the text of that line and on the [`LexState`] it *enters* with
//! (the whole stack of open constructs: a `/* ... */`, a string, an embedded language...). The
//! cache therefore stores one **entry state per line**: `entries[i]` is the state at the start of
//! line `i`, and `entries[0]` is always [`LexState::DOC_START`]. Because a [`LexState`] is a 4 byte
//! interned stack, comparing two states costs a single `u32 == u32`.
//!
//! * **Lazy**: only the entries needed to paint the viewport (plus a small lookahead) are
//!   computed, see [`SyntaxCache::advance`]. Lines below the viewport are never lexed.
//! * **Tied to the `State`**: [`SyntaxField`] is a [`StateField`], so every `State` carries its own
//!   cache and every transaction (typing, undo, redo, ...) derives a new one. The
//!   cache of a `State` is a pure memo of `f(doc)`: it only ever *adds* knowledge that is valid for
//!   that document, which is why interior mutability is sound here.
//! * **Minimal invalidation**: a transaction only invalidates the entries from the first edited
//!   line downwards. Edits below the already parsed range invalidate nothing.
//! * **Resynchronisation**: the old entries below an edit are kept as *hints* (shifted by the
//!   number of inserted/removed lines). While re-lexing, as soon as a recomputed entry state is
//!   equal to its hint, everything below is known to be unchanged and the old entries are
//!   restored without lexing them again.
//! * **Observable**: [`SyntaxCache::progress`] tells how far the parse has advanced, so jumping to
//!   line 300 000 can show "parsed up to line N".

use std::borrow::Cow;
use std::sync::{Arc, Mutex, MutexGuard};

use texera_core::{ChangeIterator, Extension, State, StateField, Text, Transaction};

use super::registry::{LangId, LanguageRegistry, LexState};

/// How far the lexical parse has advanced for a given document.
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxProgress;
///
/// let progress = SyntaxProgress { lines_ready: 50, total_lines: 100 };
/// assert!(progress.is_ready(10));
/// assert!(!progress.is_ready(60));
/// assert_eq!(progress.ratio(), 0.5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxProgress {
    /// Number of leading lines `0..lines_ready` whose entry state is known, i.e. the lines that
    /// can be highlighted exactly.
    pub lines_ready: usize,
    /// Total number of lines of the document.
    pub total_lines: usize,
}

impl SyntaxProgress {
    /// Whether the entry state of `line` (0-based) is known.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 5, total_lines: 10 };
    /// assert!(p.is_ready(4));
    /// assert!(!p.is_ready(5));
    /// ```
    pub fn is_ready(&self, line: usize) -> bool {
        line < self.lines_ready
    }

    /// Last line (0-based) that can be highlighted exactly, if any.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 5, total_lines: 10 };
    /// assert_eq!(p.last_ready_line(), Some(4));
    /// ```
    pub fn last_ready_line(&self) -> Option<usize> {
        self.lines_ready.checked_sub(1)
    }

    /// Whether the whole document has been parsed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 10, total_lines: 10 };
    /// assert!(p.is_complete());
    /// ```
    pub fn is_complete(&self) -> bool {
        self.lines_ready >= self.total_lines
    }

    /// Fraction of the document that is ready, in `0.0..=1.0`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxProgress;
    ///
    /// let p = SyntaxProgress { lines_ready: 25, total_lines: 100 };
    /// assert!((p.ratio() - 0.25).abs() < f32::EPSILON);
    /// ```
    pub fn ratio(&self) -> f32 {
        if self.total_lines == 0 {
            1.0
        } else {
            self.lines_ready as f32 / self.total_lines as f32
        }
    }
}

/// Counters describing the work done by a cache (inherited through transactions).
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxStats;
///
/// let stats = SyntaxStats::default();
/// assert_eq!(stats.lines_lexed, 0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyntaxStats {
    /// Total number of lines lexed to compute entry states.
    pub lines_lexed: u64,
    /// Number of transactions that discarded already-known entry states.
    pub invalidations: u64,
    /// Number of times an edit was absorbed by restoring old entries (resynchronisation).
    pub resyncs: u64,
}

/// Old entry states below an edit, kept as hints to resynchronise without re-lexing.
#[derive(Clone, Debug)]
struct StaleTail {
    /// Line (in the *current* document) the first hint belongs to.
    start_line: usize,
    entries: Vec<LexState>,
}

#[derive(Clone, Debug)]
struct Inner {
    /// `entries[i]` is the verified entry state of line `i`.
    entries: Vec<LexState>,
    stale: Option<StaleTail>,
    total_lines: usize,
    /// Identity of the rules the entries were computed with.
    rules_id: Option<u64>,
    stats: SyntaxStats,
}

impl Inner {
    fn new(total_lines: usize) -> Self {
        Self {
            entries: vec![LexState::DOC_START],
            stale: None,
            total_lines,
            rules_id: None,
            stats: SyntaxStats::default(),
        }
    }

    fn reset(&mut self, total_lines: usize) {
        self.entries.clear();
        self.entries.push(LexState::DOC_START);
        self.stale = None;
        self.total_lines = total_lines;
        self.rules_id = None;
        self.stats.invalidations += 1;
    }

    /// Entry states usable as resynchronisation hints for the lines `from_line..` of the document
    /// *after* an edit whose dirty region ends right before `from_line`, plus the number of
    /// lines at the start of that range for which there is **no** hint (a gap).
    ///
    /// * Verified entries are always valid hints: the lines below the edit did not change, so
    ///   if a recomputed entry equals one of them, the following ones are unchanged too.
    /// * Older hints are only reusable on their own. They are *not* chained after verified
    ///   entries, because nothing proved that the verified part ever converged to them.
    /// * When the edit lands *above* the first older hint (several edits before the parse got to
    ///   run, e.g. pressing `Enter` three times), the hints are still valid for the lines they
    ///   belonged to: they are kept and the lines in between are reported as the gap.
    fn tail_from(&self, from_line: usize) -> (Vec<LexState>, usize) {
        if from_line < self.entries.len() {
            return (self.entries[from_line..].to_vec(), 0);
        }
        if let Some(stale) = &self.stale {
            if from_line >= stale.start_line {
                if let Some(hints) = stale.entries.get(from_line - stale.start_line..) {
                    return (hints.to_vec(), 0);
                }
            } else {
                return (stale.entries.clone(), stale.start_line - from_line);
            }
        }
        (Vec::new(), 0)
    }
}

/// Shared, memoized per-line lexical state of one document version.
///
/// Cloning is `O(1)` and shares the memo (valid because both clones describe the same document).
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxCache;
///
/// let cache = SyntaxCache::new(50);
/// let progress = cache.progress();
/// assert_eq!(progress.total_lines, 50);
/// ```
#[derive(Clone, Debug)]
pub struct SyntaxCache {
    inner: Arc<Mutex<Inner>>,
}

impl SyntaxCache {
    /// Creates an empty cache for a document with `total_lines` lines.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxCache;
    ///
    /// let cache = SyntaxCache::new(20);
    /// assert_eq!(cache.progress().total_lines, 20);
    /// ```
    pub fn new(total_lines: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new(total_lines))),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Current parse progress.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxCache;
    ///
    /// let cache = SyntaxCache::new(10);
    /// let prog = cache.progress();
    /// assert_eq!(prog.lines_ready, 1); // DOC_START is ready
    /// ```
    pub fn progress(&self) -> SyntaxProgress {
        let g = self.lock();
        SyntaxProgress {
            lines_ready: g.entries.len().min(g.total_lines),
            total_lines: g.total_lines,
        }
    }

    /// Work counters, mostly useful to verify that no unnecessary line was lexed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxCache;
    ///
    /// let cache = SyntaxCache::new(10);
    /// let stats = cache.stats();
    /// assert_eq!(stats.lines_lexed, 0);
    /// ```
    pub fn stats(&self) -> SyntaxStats {
        self.lock().stats
    }

    /// Verified entry state of `line`, provided it was computed with the rules identified by
    /// `rules_id`. Returns `None` when the parse has not reached that line yet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, SyntaxCache};
    ///
    /// let registry = LanguageRegistry::new();
    /// let rules_id = registry.rules_id(0);
    /// let cache = SyntaxCache::new(5);
    /// // Line 0 is initialized with DOC_START
    /// assert!(cache.entry_state(0, rules_id).is_none()); // rules_id not set yet until advanced
    /// ```
    pub fn entry_state(&self, line: usize, rules_id: u64) -> Option<LexState> {
        let g = self.lock();
        if g.rules_id != Some(rules_id) {
            return None;
        }
        g.entries.get(line).copied()
    }

    /// Copy of every verified entry state (`result[i]` is the entry state of line `i`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LexState, SyntaxCache};
    ///
    /// let cache = SyntaxCache::new(1);
    /// assert_eq!(cache.known_states(), vec![LexState::DOC_START]);
    /// ```
    pub fn known_states(&self) -> Vec<LexState> {
        self.lock().entries.clone()
    }

    /// Makes the entry states of lines `0..=through_line` known, lexing **at most `budget`
    /// lines** from the parse frontier. Lines already known are never lexed again.
    ///
    /// If `budget` is not enough to reach `through_line` (e.g. the user jumped to line 300 000),
    /// the frontier simply advances as far as the budget allows; call again (typically once per
    /// frame) until [`SyntaxProgress::is_ready`] reports the target line.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::{LanguageRegistry, SyntaxCache};
    /// use texera_core::Text;
    ///
    /// let registry = LanguageRegistry::new();
    /// registry.register("toy", "[meta]\nname = \"toy\"\n[states.root]\nrules = []").unwrap();
    /// let doc = Text::from("hello\nworld");
    /// let cache = SyntaxCache::new(doc.len_lines());
    /// let prog = cache.advance(&doc, &registry, 0, 1, 10);
    /// assert_eq!(prog.lines_ready, 2);
    /// ```
    pub fn advance(
        &self,
        doc: &Text,
        registry: &LanguageRegistry,
        lang: LangId,
        through_line: usize,
        budget: usize,
    ) -> SyntaxProgress {
        let rules_id = registry.rules_id(lang);
        let mut guard = self.lock();
        let g: &mut Inner = &mut guard;
        let total = doc.len_lines();
        if g.total_lines != total || g.rules_id.is_some_and(|id| id != rules_id) {
            g.reset(total);
        }
        g.rules_id = Some(rules_id);

        let wanted = through_line.saturating_add(1).min(total);
        let mut lexed = 0usize;

        if g.entries.len() < wanted && budget > 0 {
            // Entry `n` is computed by lexing line `n - 1` from entry `n - 1`.
            let mut lines = doc.lines_at(g.entries.len() - 1);
            while g.entries.len() < wanted && lexed < budget {
                let Some(slice) = lines.next() else { break };
                let n = g.entries.len();
                let text: Cow<str> = match slice.as_str() {
                    Some(s) => Cow::Borrowed(s),
                    None => Cow::Owned(slice.to_string()),
                };
                let exit = registry.scan_line(lang, trim_line_break(&text), g.entries[n - 1]);
                lexed += 1;
                g.stats.lines_lexed += 1;

                // Resynchronise with the old entries kept below the last edit.
                let mut restored = None;
                if let Some(stale) = &g.stale {
                    if n >= stale.start_line {
                        let k = n - stale.start_line;
                        if k >= stale.entries.len() {
                            g.stale = None;
                        } else if stale.entries[k] == exit {
                            restored = Some(stale.entries[k..].to_vec());
                        }
                    }
                }
                if let Some(restored) = restored {
                    g.entries.extend(restored);
                    g.stale = None;
                    g.stats.resyncs += 1;
                    break;
                }
                g.entries.push(exit);
            }
        }

        // The registry may have been invalidated while lexing (state pool flushed, a language
        // registered from another thread): what was computed is meaningless, start over.
        if registry.rules_id(lang) != rules_id {
            g.reset(total);
        }

        SyntaxProgress {
            lines_ready: g.entries.len().min(g.total_lines),
            total_lines: g.total_lines,
        }
    }

    /// Derives the cache of the document resulting from `tr`.
    ///
    /// * No document change: the very same memo is shared.
    /// * Otherwise only entries after the first edited line are discarded, and the old entries
    ///   below the last edited line are kept as resynchronisation hints.
    pub(crate) fn after_transaction(&self, tr: &Transaction) -> SyntaxCache {
        if !tr.doc_changed() {
            return self.clone();
        }

        let changes = tr.changes();
        let (mut first_old, mut last_old, mut last_new) = (usize::MAX, 0usize, 0usize);
        for change in changes.description().iter_changes(false) {
            first_old = first_old.min(change.old.from);
            last_old = last_old.max(change.old.to);
            last_new = last_new.max(change.new.to);
        }
        if first_old == usize::MAX {
            return self.clone();
        }

        let old_doc = tr.start_state().doc();
        let new_doc = tr.doc();
        // Dirty region: old lines `first_line..=old_last_line` became `first_line..=new_last_line`.
        let first_line = old_doc.char_to_line(first_old);
        let old_last_line = old_doc.char_to_line(last_old);
        let new_last_line = new_doc.char_to_line(last_new);

        let g = self.lock();
        // Entry of line `first_line` only depends on the lines above it: still valid.
        let keep = g.entries.len().min(first_line + 1);
        let (tail, gap) = g.tail_from(old_last_line + 1);

        let mut stats = g.stats;
        if keep < g.entries.len() {
            stats.invalidations += 1;
        }

        let inner = Inner {
            entries: g.entries[..keep].to_vec(),
            stale: (!tail.is_empty()).then(|| StaleTail {
                start_line: new_last_line + 1 + gap,
                entries: tail,
            }),
            total_lines: new_doc.len_lines(),
            rules_id: g.rules_id,
            stats,
        };
        SyntaxCache {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

fn trim_line_break(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}

/// [`StateField`] holding the [`SyntaxCache`] of a `State`.
///
/// Register it with [`SyntaxField::extension`] (or `HighlightPlugin::syntax_extension()`) so the
/// highlighter can cache its parse per document version and follow undo / redo.
///
/// # Examples
///
/// ```rust
/// use texera::SyntaxField;
/// use texera_core::State;
///
/// let state = State::from("hello").with_extensions([SyntaxField::extension()]);
/// ```
pub struct SyntaxField;

impl StateField for SyntaxField {
    type Value = SyntaxCache;

    fn create(state: &State) -> SyntaxCache {
        SyntaxCache::new(state.doc().len_lines())
    }

    fn update(value: &SyntaxCache, tr: &Transaction) -> SyntaxCache {
        value.after_transaction(tr)
    }
}

impl SyntaxField {
    /// Extension registering the field in a `State`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use texera::SyntaxField;
    /// use texera_core::State;
    ///
    /// let state = State::from("hello").with_extensions([SyntaxField::extension()]);
    /// ```
    pub fn extension() -> Extension {
        <Self as StateField>::init()
    }
}
