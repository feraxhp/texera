#![allow(dead_code)]

//! Helpers shared by the syntax engine tests.

use std::ops::Range;
use std::sync::mpsc;
use std::time::Duration;

use texera::{LangId, LanguageRegistry, LexState};

pub const JSON: &str = include_str!("../../syntaxes/json.toml");
pub const RUST: &str = include_str!("../../syntaxes/rust.toml");
pub const MARKDOWN: &str = include_str!("../../syntaxes/markdown.toml");

/// A registry with `langs` registered; the first one is the language lexing starts in.
pub fn registry(langs: &[(&str, &str)]) -> (LanguageRegistry, LangId) {
    let reg = LanguageRegistry::new();
    let mut first = None;
    for (name, source) in langs {
        let registered = reg
            .register(name, source)
            .unwrap_or_else(|e| panic!("`{name}` must compile: {e}"));
        first.get_or_insert(registered.lang_id);
    }
    (reg, first.expect("at least one language"))
}

/// One lexed token: its text and its token name.
pub type Tok = (String, String);

/// Lexes `lines` in order threading the state; returns the tokens and the exit state of each line.
pub fn lex(reg: &LanguageRegistry, lang: LangId, lines: &[&str]) -> Vec<(Vec<Tok>, LexState)> {
    let mut state = LexState::DOC_START;
    lines
        .iter()
        .map(|line| {
            let tokens = reg.tokenize_line_stateful(lang, line, &mut state);
            let toks = tokens
                .into_iter()
                .map(|(r, _lang, t)| (line[r].to_string(), t.to_string()))
                .collect();
            (toks, state)
        })
        .collect()
}

/// Lexes `lines`, returns only the tokens of each line.
pub fn tokens(reg: &LanguageRegistry, lang: LangId, lines: &[&str]) -> Vec<Vec<Tok>> {
    lex(reg, lang, lines).into_iter().map(|(t, _)| t).collect()
}

pub fn tok(text: &str, token: &str) -> Tok {
    (text.to_string(), token.to_string())
}

/// Ranges and token names of the single line `line` lexed from the base state.
pub fn ranged(reg: &LanguageRegistry, lang: LangId, line: &str) -> Vec<(Range<usize>, String)> {
    reg.tokenize_line(lang, line)
        .into_iter()
        .map(|(r, _lang, t)| (r, t.to_string()))
        .collect()
}

pub fn has(toks: &[Tok], text: &str, token: &str) -> bool {
    toks.iter().any(|(t, k)| t == text && k == token)
}

/// Token assigned to `text` (first occurrence), if any.
pub fn token_of<'a>(toks: &'a [Tok], text: &str) -> Option<&'a str> {
    toks.iter().find(|(t, _)| t == text).map(|(_, k)| k.as_str())
}

/// Runs `f` on another thread and fails the test if it does not finish in time (infinite loops).
pub fn with_timeout<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(secs))
        .expect("the lexer did not finish in time (infinite loop?)")
}

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as usize
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.next() % n.max(1)
    }
}
