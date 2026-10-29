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

pub mod compiler;
pub mod raw;
pub mod types;

pub use types::DOC_START_STATE_ID;
pub use types::CompiledLanguage;
pub use types::DOC_START_STATE;
pub use types::ROOT_STATE_ID;
pub use types::SyntaxError;
pub use types::ROOT_STATE;
pub use compiler::compile;
pub use types::TokenName;

pub(crate) use types::EmbedName;
pub(crate) use types::EmbedEnd;
pub(crate) use types::Action;
