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

//! Incremental syntax highlighting driven by declarative TOML state machines.
//!
//! * [`lang`] — the TOML format and its compiler.
//! * [`registry`] — [`LanguageRegistry`]: compiled languages, interned state stacks and the line
//!   lexer ([`LexState`] is a 4 byte handle to the stack of open constructs).
//! * [`syntax_cache`] — per-document incremental cache of the entry state of every line.
//! * [`theme`] — [`ThemeMap`] for syntax color assignments.
//! * [`spec`] — language specification definitions and filesystem loaders.
//! * [`plugin`] — [`HighlightPlugin`] implementation for the editor view.
//!
//! See [`HighlightPlugin`] for the plugin plugged into the editor view.

mod _impls;
mod extend;
mod lexer;

pub mod lang;
pub mod plugin;
pub mod pool;
pub mod progress;
pub mod registry;
pub mod spec;
pub mod syntax_cache;
pub mod theme;

pub use lang::DOC_START_STATE_ID;
pub use lang::CompiledLanguage;
pub use lang::DOC_START_STATE;
pub use lang::ROOT_STATE_ID;
pub use lang::SyntaxError;
pub use lang::ROOT_STATE;
pub use lang::TokenName;

pub use registry::LanguageRegistry;
pub use registry::MAX_POOL_STATES;
pub use registry::ScopedToken;
pub use registry::PLAIN_LANG;
pub use registry::Registered;
pub use registry::StateFrame;
pub use registry::MAX_DEPTH;
pub use registry::LexState;
pub use registry::LangId;
pub use registry::Loader;

pub use syntax_cache::SyntaxProgress;
pub use syntax_cache::SyntaxCache;
pub use syntax_cache::SyntaxField;
pub use syntax_cache::SyntaxStats;

pub use plugin::DEFAULT_LOOKAHEAD_LINES;
pub use plugin::DEFAULT_PARSE_BUDGET;
pub use plugin::HighlightPlugin;

pub use spec::dir_spec_loader;
pub use spec::LanguageSpec;
pub use spec::LanguageToml;
pub use spec::dir_loader;

pub use theme::ThemeMap;

/// Convenience macro to embed a single language's TOML and optional theme.json at compile-time.
///
/// # Examples
///
/// ```rust,ignore
/// use texera::include_toml_language;
///
/// let lang = include_toml_language!("rust", "../syntaxes/rust.toml");
/// ```
#[macro_export]
macro_rules! include_toml_language {
    ($lang:expr, $toml_file:expr) => {
        $crate::LanguageToml::new(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::default(),
        )
    };
    ($lang:expr, $toml_file:expr, $json_file:expr) => {
        $crate::LanguageToml::new(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::from_json_str(include_str!($json_file)).unwrap_or_default(),
        )
    };
    ($highlighter:expr, $lang:expr, $toml_file:expr) => {
        $highlighter.with_language($lang, include_str!($toml_file), None)
    };
    ($highlighter:expr, $lang:expr, $toml_file:expr, $json_file:expr) => {
        $highlighter.with_language(
            $lang,
            include_str!($toml_file),
            $crate::ThemeMap::from_json_str(include_str!($json_file)).ok(),
        )
    };
}

/// Backwards compatibility alias for [`include_toml_language!`].
#[macro_export]
macro_rules! include_scm_language {
    ($($arg:tt)*) => {
        $crate::include_toml_language!($($arg)*)
    };
}
